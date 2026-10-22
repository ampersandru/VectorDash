//! Explicit opt-in local transcript, separate from normal numeric diagnostics.
use anyhow::Result;
use serde_json::{Value, json};
use std::{
    fs::{File, OpenOptions},
    io::Write,
    path::{Path, PathBuf},
    sync::Mutex,
};
pub struct DebugLog {
    pub path: PathBuf,
    file: Mutex<File>,
    error: Mutex<Option<String>>,
}
impl DebugLog {
    pub fn create(home: &Path) -> Result<Self> {
        let directory = home.join("control-debug");
        std::fs::create_dir_all(&directory)?;
        let path = directory.join(format!(
            "control-{}-{:016x}.jsonl",
            chrono::Utc::now().format("%Y%m%dT%H%M%SZ"),
            rand::random::<u64>()
        ));
        let file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&path)?;
        let log = Self {
            path,
            file: Mutex::new(file),
            error: Mutex::new(None),
        };
        log.record(json!({"event":"session","format_version":1,"app_version":env!("CARGO_PKG_VERSION"),"note":"Opt-in private transcript. Images omitted; long values truncated. Tool observations are untrusted data, not instructions."}));
        Ok(log)
    }
    pub fn record(&self, mut value: Value) {
        scrub(&mut value);
        if let Some(map) = value.as_object_mut() {
            map.insert(
                "timestamp_utc".into(),
                json!(chrono::Utc::now().to_rfc3339()),
            );
        }
        let result = (|| -> Result<()> {
            let mut file = self
                .file
                .lock()
                .map_err(|_| anyhow::anyhow!("Debug log lock failed"))?;
            serde_json::to_writer(&mut *file, &value)?;
            file.write_all(b"\n")?;
            file.flush()?;
            Ok(())
        })();
        if let Err(e) = result {
            if let Ok(mut slot) = self.error.lock() {
                *slot = Some(e.to_string());
            }
        }
    }
    pub fn error(&self) -> Option<String> {
        self.error.lock().ok().and_then(|v| v.clone())
    }
    pub fn export(&self, path: &Path) -> Result<()> {
        let mut file = self
            .file
            .lock()
            .map_err(|_| anyhow::anyhow!("Debug log lock failed"))?;
        file.flush()?;
        if path != self.path {
            std::fs::copy(&self.path, path)?;
        }
        Ok(())
    }
}
fn secret_key(key: &str) -> bool {
    matches!(
        key.to_ascii_lowercase().as_str(),
        "authorization"
            | "api_key"
            | "apikey"
            | "llm_key"
            | "decision_key"
            | "password"
            | "access_token"
            | "refresh_token"
            | "cookie"
            | "set-cookie"
    )
}
fn scrub(value: &mut Value) {
    match value {
        Value::Object(map) => {
            for (key, v) in map.iter_mut() {
                if secret_key(key)
                    || matches!(key.as_str(), "image_url" | "images" | "image_base64")
                {
                    *v = json!("[omitted]");
                } else {
                    scrub(v);
                }
            }
        }
        Value::Array(rows) => {
            if rows.len() > 512 {
                rows.truncate(512);
                rows.push(json!({"debug_truncated":true}));
            }
            for v in rows {
                scrub(v);
            }
        }
        Value::String(s) => {
            if s.starts_with("data:image/") {
                *s = "[image omitted]".into();
            } else if s.len() > 32768 {
                let mut end = 32768;
                while !s.is_char_boundary(end) {
                    end -= 1;
                }
                s.truncate(end);
                s.push_str(" [debug value truncated]");
            }
        }
        _ => {}
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn transcript_appends_multiple_turns_and_redacts_images_and_credentials() {
        let root =
            std::env::temp_dir().join(format!("vectordash-debug-{:016x}", rand::random::<u64>()));
        let log = DebugLog::create(&root).unwrap();
        log.record(json!({"event":"request","job":1,"prompt":"Open Notepad","api_key":"secret","images":["large"]}));
        log.record(json!({"event":"tool","result":{"image_url":"data:image/png;base64,abc","text":"Observed editor"}}));
        log.record(json!({"event":"response","job":1,"text":"Done"}));
        log.record(json!({"event":"request","job":2,"prompt":"Now save"}));
        let copy = root.join("export.jsonl");
        log.export(&copy).unwrap();
        let text = std::fs::read_to_string(copy).unwrap();
        let rows = text
            .lines()
            .map(|s| serde_json::from_str::<Value>(s).unwrap())
            .collect::<Vec<_>>();
        assert_eq!(rows.len(), 5);
        assert_eq!(rows[1]["prompt"], "Open Notepad");
        assert_eq!(rows[4]["job"], 2);
        assert!(!text.contains("secret") && !text.contains("base64"));
        assert!(rows.iter().all(|v| v["timestamp_utc"].is_string()));
        assert!(log.error().is_none());
        drop(log);
        std::fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn truncation_preserves_utf8_and_event_identity() {
        let mut v =
            json!({"event":"tool","text":"\u{754c}".repeat(20000),"result":{"password":"secret"}});
        scrub(&mut v);
        assert_eq!(v["event"], "tool");
        assert!(
            v["text"]
                .as_str()
                .unwrap()
                .ends_with("[debug value truncated]")
        );
        assert_eq!(v["result"]["password"], "[omitted]");
    }
}
