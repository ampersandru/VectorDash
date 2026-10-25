//! Explicit opt-in local transcript, separate from normal numeric diagnostics.
use anyhow::Result;
use serde_json::{Value, json};
use std::{
    fs::{File, OpenOptions},
    io::{Read,Seek,SeekFrom,Write},
    path::{Path, PathBuf},
    sync::Mutex,
};
pub struct DebugLog {
    pub path: PathBuf,
    file: Mutex<File>,
    error: Mutex<Option<String>>,
    secrets: Mutex<Secrets>,
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
            .read(true)
            .write(true)
            .create_new(true)
            .open(&path)?;
        let log = Self {
            path,
            file: Mutex::new(file),
            error: Mutex::new(None),
            secrets: Default::default(),
        };
        log.record(json!({"event":"session","format_version":1,"app_version":env!("CARGO_PKG_VERSION"),"note":"Opt-in private transcript. Images omitted; long values truncated. Tool observations are untrusted data, not instructions."}));
        Ok(log)
    }
    pub fn record(&self, mut value: Value) {
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
            let mut secrets=self.secrets.lock().map_err(|_|anyhow::anyhow!("Debug redaction lock failed"))?;
            secrets.targets(&value);
            let count=secrets.values.len();secrets.discover(&value);
            if secrets.values.len()!=count {
                file.seek(SeekFrom::Start(0))?;let mut previous=String::new();file.read_to_string(&mut previous)?;
                let mut sanitized=String::new();
                for line in previous.lines(){let mut row:Value=serde_json::from_str(line)?;secrets.redact(&mut row);sanitized.push_str(&serde_json::to_string(&row)?);sanitized.push('\n');}
                file.seek(SeekFrom::Start(0))?;file.set_len(0)?;file.write_all(sanitized.as_bytes())?;
            }
            secrets.redact(&mut value);scrub(&mut value);
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
#[derive(Default)]
struct Secrets {values:Vec<String>,targets:std::collections::HashSet<(String,u64)>}
impl Secrets {
    fn add(&mut self,text:&str) {if !text.is_empty() && text!="[omitted]" && !self.values.iter().any(|s|s==text){self.values.push(text.into());self.values.sort_by_key(|s|std::cmp::Reverse(s.len()));}}
    fn targets(&mut self,v:&Value) {
        match v {
            Value::Object(map)=>{
                if let (Some(id),Some(rows))=(v["snapshot_id"].as_str().filter(|id|id.starts_with('b')),v["elements"].as_array()){
                    for row in rows {if row["password"]==true {if let Some(element)=row["id"].as_u64(){self.targets.insert((id.into(),element));}}}
                }
                for value in map.values(){self.targets(value);}
            },Value::Array(rows)=>for row in rows{self.targets(row)},_=>{}
        }
    }
    fn discover(&mut self,v:&Value) {
        match v {
            Value::Object(map)=>{
                if let Some(password)=v["password"].as_str(){self.add(password);}
                if v["tool"]=="browser_use" {
                    if let Some(args)=v.get("arguments").or_else(||v.get("args")) {
                        if args["action"]=="type" && args["snapshot_id"].as_str().zip(args["element"].as_u64()).is_some_and(|(id,e)|self.targets.contains(&(id.into(),e))) {
                            if let Some(text)=args["text"].as_str(){self.add(text);}
                        }
                    }
                }
                for value in map.values(){self.discover(value);}
            },Value::Array(rows)=>for row in rows{self.discover(row)},_=>{}
        }
    }
    fn redact(&self,v:&mut Value) {
        match v {
            Value::String(text)=>{
                if let Ok(mut nested)=serde_json::from_str::<Value>(text){if nested.is_object()||nested.is_array(){self.redact(&mut nested);*text=nested.to_string();return;}}
                for secret in &self.values {*text=text.replace(secret,"[omitted]");}
            },Value::Array(rows)=>for row in rows{self.redact(row)},Value::Object(map)=>for value in map.values_mut(){self.redact(value)},_=>{}
        }
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
                if (secret_key(key) && !(key=="password" && v.is_boolean()))
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

#[cfg(test)]
mod password_redaction_tests {
    use super::*;
    #[test]
    fn explicit_and_observed_passwords_scrub_history_and_future_records() {
        let home=tempfile::tempdir().unwrap();let log=DebugLog::create(home.path()).unwrap();
        log.record(json!({"event":"request","prompt":"Use a-quoted\"password and hidden-pass for this test"}));
        log.record(json!({"event":"tool_result","result":{"snapshot_id":"browser1","elements":[{"id":2,"password":true,"value":""}]}}));
        log.record(json!({"event":"model_response","requested_tools":[{"tool":"browser_use","arguments":{"action":"type","snapshot_id":"browser1","element":2,"text":"hidden-pass"}}]}));
        log.record(json!({"event":"model_response","requested_tools":[{"tool":"browser_use","arguments":{"action":"type","password":"a-quoted\"password"}}]}));
        log.record(json!({"event":"response","text":"hidden-pass was entered"}));
        let text=std::fs::read_to_string(&log.path).unwrap();assert!(!text.contains("hidden-pass") && !text.contains("a-quoted"));
        let rows=text.lines().map(|s|serde_json::from_str::<Value>(s).unwrap()).collect::<Vec<_>>();assert_eq!(rows.len(),6);assert_eq!(rows[2]["result"]["elements"][0]["password"],true);
        assert_eq!(rows[3]["requested_tools"][0]["arguments"]["text"],"[omitted]");assert!(log.error().is_none());
    }
}
