use crate::{config::Settings, content::Segment, engine::Control};
use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::{
    io::{BufRead, BufReader, Write},
    path::Path,
    process::{Command, Stdio},
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};
#[derive(Serialize, Deserialize)]
pub struct Request {
    pub path: String,
    pub settings: Settings,
    #[serde(default)]
    pub start_page: u32,
}
pub fn worker_main() -> Result<()> {
    crate::platform::lower_integrity()?;
    let mut line = String::new();
    std::io::stdin().lock().read_line(&mut line)?;
    anyhow::ensure!(line.len() < 1024 * 1024, "Worker request too large");
    let request: Request = serde_json::from_str(&line)?;
    let result = crate::content::stream_from(
        Path::new(&request.path),
        request.start_page,
        &request.settings,
        &|| Ok(()),
        &mut |segment| {
            println!("{}", serde_json::json!({"segment":segment}));
            Ok(())
        },
    );
    match result {
        Ok(()) => println!("{}", serde_json::json!({"done":true})),
        Err(e) => println!("{}", serde_json::json!({"error":e.to_string()})),
    }

    Ok(())
}
pub fn visit(
    path: &Path,
    s: &Settings,
    control: &Control,
    consume: &mut dyn FnMut(Segment) -> Result<()>,
) -> Result<()> {
    visit_checked(path, s, control, 0, &|| false, consume)
}
pub fn visit_checked(
    path: &Path,
    s: &Settings,
    control: &Control,
    start_page: u32,
    stopped: &dyn Fn() -> bool,
    consume: &mut dyn FnMut(Segment) -> Result<()>,
) -> Result<()> {
    // Unit tests run a harness EXE, not the worker-capable application EXE.
    #[cfg(test)]
    {
        return crate::content::stream_from(
            path,
            start_page,
            s,
            &|| {
                anyhow::ensure!(!stopped(), "Preparation stopped");
                control.file_checkpoint()
            },
            consume,
        );
    }
    let _raw_permit = if crate::raw::is_raw(&crate::content::extension(path)) {
        Some(crate::raw::serial(&|| control.check())?)
    } else {
        None
    };
    let mut settings = s.clone();
    settings.llm_key.clear();
    settings.tts_key.clear();
    settings.remote_key.clear();
    settings.dictation_key.clear();
    settings.hf_token.clear();
    settings.github_token.clear();
    settings.mcp_token.clear();
    settings.llm_key.clear();
    settings.tts_key.clear();
    let mut command = Command::new(std::env::current_exe()?);
    command
        .arg("--extract-worker")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null());
    crate::content::hidden(&mut command);
    let mut child = command.spawn()?;
    let job = crate::platform::limit_worker(&child)?;
    let request = Request {
        path: path.to_string_lossy().into_owned(),
        settings,
        start_page,
    };
    let mut input = child.stdin.take().context("Worker input unavailable")?;
    input.write_all(serde_json::to_string(&request)?.as_bytes())?;
    input.write_all(b"\n")?;
    drop(input);
    let output = child.stdout.take().context("Worker output unavailable")?;
    let (tx, rx) = crossbeam_channel::bounded::<String>(4);
    std::thread::spawn(move || {
        let mut reader = BufReader::new(output);
        loop {
            let mut bytes = vec![];
            match reader
                .by_ref()
                .take(16 * 1024 * 1024 + 1)
                .read_until(b'\n', &mut bytes)
            {
                Ok(0) => break,
                Ok(_) => {
                    if bytes.len() > 16 * 1024 * 1024 {
                        let _ = tx.send("{\"error\":\"Parser message exceeded limit\"}".into());
                        break;
                    }
                    if tx
                        .send(String::from_utf8_lossy(&bytes).into_owned())
                        .is_err()
                    {
                        break;
                    }
                }
                Err(_) => break,
            }
        }
    });
    let started = Instant::now();

    let result = (|| -> Result<()> {
        loop {
            anyhow::ensure!(!stopped(), "Preparation stopped");
            control.check_with(stopped)?;
            anyhow::ensure!(
                started.elapsed() < Duration::from_secs(900),
                "Parser exceeded 15-minute limit"
            );
            match rx.recv_timeout(Duration::from_millis(50)) {
                Ok(line) => {
                    let value: serde_json::Value = serde_json::from_str(&line)?;
                    if let Some(error) = value["error"].as_str() {
                        anyhow::bail!("{error}");
                    }
                    if value["done"] == true {
                        break;
                    }
                    if value.get("segment").is_some() {
                        let segment: Segment = serde_json::from_value(value["segment"].clone())?;
                        consume(segment)?;
                    }
                }
                Err(crossbeam_channel::RecvTimeoutError::Timeout) => continue,
                Err(_) => anyhow::bail!("Parser exited without completing the file"),
            }
        }
        Ok(())
    })();
    let _ = child.kill();
    let _ = child.wait();
    drop(job);
    result?;
    Ok(())
}
use std::io::Read;
