use crate::config::Settings;
use anyhow::{Context, Result};
use serde_json::{Value, json};
use std::{
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    time::Duration,
};

pub fn voice_ids(v: &Value) -> Vec<String> {
    let array = v
        .as_array()
        .or_else(|| v["voices"].as_array())
        .or_else(|| v["data"].as_array());
    let mut ids = array
        .into_iter()
        .flatten()
        .filter_map(|v| {
            v.as_str()
                .or_else(|| v["id"].as_str())
                .or_else(|| v["voice_id"].as_str())
                .or_else(|| v["name"].as_str())
        })
        .take(1000)
        .map(str::to_owned)
        .collect::<Vec<_>>();
    ids.sort();
    ids.dedup();
    ids
}
pub fn catalog(s: &Settings) -> Result<(Vec<String>, Vec<String>, String)> {
    let mut config = s.clone();
    config.llm_url = s.tts_url.clone();
    config.llm_key = s.tts_key.clone();
    let models = crate::ai::models(&config)?;
    let agent: ureq::Agent = ureq::Agent::config_builder()
        .timeout_global(Some(Duration::from_secs(15)))
        .max_redirects(0)
        .http_status_as_error(false)
        .build()
        .into();
    for path in ["audio/voices", "voices"] {
        let mut req = agent.get(crate::ai::endpoint(&s.tts_url, path)?);
        if !s.tts_key.trim().is_empty() {
            req = req.header("Authorization", format!("Bearer {}", s.tts_key.trim()));
        }
        let mut response = req.call().context("Could not refresh TTS voices")?;
        if response.status().as_u16() == 404 || response.status().as_u16() == 405 {
            continue;
        }
        anyhow::ensure!(
            response.status().is_success(),
            "TTS voice list returned HTTP {}",
            response.status()
        );
        let data: Value = response
            .body_mut()
            .with_config()
            .limit(1024 * 1024)
            .read_json()?;
        let voices = voice_ids(&data);
        if !voices.is_empty() {
            return Ok((models, voices, String::new()));
        }
    }
    Ok((
        models,
        vec![],
        "This server does not publish a voice list. Enter its voice ID below.".into(),
    ))
}
pub fn speech_parts(markdown: &str) -> Vec<String> {
    use pulldown_cmark::{Event, Parser};
    let mut plain = String::new();
    for event in Parser::new(markdown) {
        match event {
            Event::Text(t) | Event::Code(t) => plain.push_str(&t),
            Event::SoftBreak | Event::HardBreak | Event::End(_) => plain.push(' '),
            _ => {}
        }
    }
    let plain = plain.split_whitespace().collect::<Vec<_>>().join(" ");
    let mut parts = vec![];
    let mut part = String::new();
    let mut count = 0;
    for word in plain.split_inclusive(' ') {
        for ch in word.chars() {
            if count >= 4000 {
                parts.push(std::mem::take(&mut part));
                count = 0;
            }
            part.push(ch);
            count += 1;
        }
    }
    if !part.trim().is_empty() {
        parts.push(part);
    }
    parts
}
pub fn synthesize(s: &Settings, text: &str, cancel: &Arc<AtomicBool>) -> Result<Vec<u8>> {
    anyhow::ensure!(
        !s.tts_model.trim().is_empty() && !s.tts_voice.trim().is_empty(),
        "Select a TTS model and voice in AI settings"
    );
    let url = crate::ai::endpoint(&s.tts_url, "audio/speech")?;
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()?;
    runtime.block_on(async {
  let client=reqwest::Client::builder().connect_timeout(Duration::from_secs(5)).timeout(Duration::from_secs(120)).redirect(reqwest::redirect::Policy::none()).build()?;
  let mut req=client.post(url).json(&json!({"model":s.tts_model,"voice":s.tts_voice,"input":text,"response_format":"wav"}));if !s.tts_key.trim().is_empty(){req=req.bearer_auth(s.tts_key.trim());}
  let cancellation=async {while !cancel.load(Ordering::Acquire){tokio::time::sleep(Duration::from_millis(25)).await;}};tokio::pin!(cancellation);
  let mut response=tokio::select!{biased;_=&mut cancellation=>anyhow::bail!("Read aloud stopped"),r=req.send()=>r.context("Could not connect to the TTS server")?};
  anyhow::ensure!(response.status().is_success(),"TTS server returned HTTP {}",response.status());
  let mut audio=vec![];loop {let bytes=tokio::select!{biased;_=&mut cancellation=>anyhow::bail!("Read aloud stopped"),r=response.chunk()=>r?};let Some(bytes)=bytes else {break};anyhow::ensure!(audio.len()+bytes.len()<=32*1024*1024,"TTS audio exceeded the limit");audio.extend_from_slice(&bytes);}
  anyhow::ensure!(audio.starts_with(b"RIFF")&&audio.get(8..12)==Some(b"WAVE"),"TTS server did not return WAV audio");Ok(audio)
 })
}
pub fn speak(s: &Settings, text: &str, cancel: &Arc<AtomicBool>) -> Result<()> {
    use std::io::Write;
    let parts = speech_parts(text);
    anyhow::ensure!(!parts.is_empty(), "No answer to read aloud");
    for part in parts {
        if cancel.load(Ordering::Acquire) {
            break;
        }
        let bytes = synthesize(s, &part, cancel)?;
        if cancel.load(Ordering::Acquire) {
            break;
        }
        let mut file = tempfile::Builder::new()
            .prefix("vectordash-tts-")
            .suffix(".wav")
            .tempfile()?;
        file.write_all(&bytes)?;
        file.flush()?;
        crate::platform::play_at(file.path().to_str().context("Invalid audio path")?, 0.0)?;
        loop {
            let errors = crate::platform::playback_errors();
            anyhow::ensure!(errors.is_empty(), "{}", errors.join("; "));
            if !crate::platform::playback_active() {
                break;
            }
            if cancel.load(Ordering::Acquire) {
                crate::platform::stop_playback();
                break;
            }
            std::thread::sleep(Duration::from_millis(100));
        }
    }
    Ok(())
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn voices_and_long_markdown() {
        assert_eq!(
            voice_ids(&json!({"voices":["af_heart",{"id":"nova"},{"voice_id":"custom"}]})),
            vec!["af_heart", "custom", "nova"]
        );
        let parts = speech_parts(&format!(
            "**Hello** [world](https://example.org) {}",
            "犬".repeat(9000)
        ));
        assert!(parts.iter().all(|s| s.chars().count() <= 4000));
        assert!(!parts.join("").contains("https://"));
        assert!(parts.join("").starts_with("Hello world"));
    }
    #[test]
    fn configured_auth_and_wav() {
        let server = tiny_http::Server::http("127.0.0.1:0").unwrap();
        let s = Settings {
            tts_url: format!("http://{}/v1", server.server_addr()),
            tts_key: "test-token".into(),
            tts_model: "tts-test".into(),
            tts_voice: "voice-test".into(),
            ..Default::default()
        };
        let worker =
            std::thread::spawn(move || {
                let mut req = server.recv().unwrap();
                assert_eq!(req.url(), "/v1/audio/speech");
                assert!(
                    req.headers().iter().any(|h| h.field.equiv("Authorization")
                        && h.value.as_str() == "Bearer test-token")
                );
                let mut body = String::new();
                req.as_reader().read_to_string(&mut body).unwrap();
                let body: Value = serde_json::from_str(&body).unwrap();
                assert_eq!(body["voice"], "voice-test");
                assert_eq!(body["response_format"], "wav");
                req.respond(tiny_http::Response::from_data(
                    b"RIFF0000WAVEfmt test".to_vec(),
                ))
                .unwrap();
            });
        assert!(
            synthesize(&s, "Hello", &Arc::new(AtomicBool::new(false)))
                .unwrap()
                .starts_with(b"RIFF")
        );
        worker.join().unwrap();
    }
}

#[cfg(test)]
mod catalog_tests {
    use super::*;
    #[test]
    fn refresh_falls_back_to_voices_and_preserves_auth() {
        let server = tiny_http::Server::http("127.0.0.1:0").unwrap();
        let s = Settings {
            tts_url: format!("http://{}/v1", server.server_addr()),
            tts_key: "secret".into(),
            ..Default::default()
        };
        let worker =
            std::thread::spawn(move || {
                for (path, status, body) in [
                    ("/v1/models", 200, r#"{"data":[{"id":"tts-test"}]}"#),
                    ("/v1/audio/voices", 404, "{}"),
                    ("/v1/voices", 200, r#"{"voices":["af_heart","af_bella"]}"#),
                ] {
                    let req = server.recv().unwrap();
                    assert_eq!(req.url(), path);
                    assert!(
                        req.headers().iter().any(|h| h.field.equiv("Authorization")
                            && h.value.as_str() == "Bearer secret")
                    );
                    req.respond(tiny_http::Response::from_string(body).with_status_code(status))
                        .unwrap();
                }
            });
        let (models, voices, notice) = catalog(&s).unwrap();
        assert_eq!(models, vec!["tts-test"]);
        assert_eq!(voices, vec!["af_bella", "af_heart"]);
        assert!(notice.is_empty());
        worker.join().unwrap();
    }
    #[test]
    fn cancellation_does_not_wait_for_audio() {
        let server = tiny_http::Server::http("127.0.0.1:0").unwrap();
        let s = Settings {
            tts_url: format!("http://{}/v1", server.server_addr()),
            tts_model: "test".into(),
            tts_voice: "test".into(),
            ..Default::default()
        };
        let start = std::time::Instant::now();
        assert!(synthesize(&s, "hello", &Arc::new(AtomicBool::new(true))).is_err());
        assert!(start.elapsed() < Duration::from_secs(1));
    }
    #[test]
    fn tts_key_is_protected_in_settings() {
        let home = tempfile::tempdir().unwrap();
        let s = Settings {
            tts_key: "tts-secret-key".into(),
            ..Default::default()
        };
        s.save(home.path()).unwrap();
        assert!(
            !std::fs::read_to_string(home.path().join("settings.json"))
                .unwrap()
                .contains("tts-secret-key")
        );
        assert_eq!(
            Settings::load(home.path()).unwrap().tts_key,
            "tts-secret-key"
        );
    }
}
