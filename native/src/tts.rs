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
    let (voices, notice) = voices(s).unwrap_or_else(|e| (vec![], format!("Voice refresh: {e}")));
    Ok((models, voices, notice))
}
pub fn voices(s: &Settings) -> Result<(Vec<String>, String)> {
    let agent: ureq::Agent = ureq::Agent::config_builder()
        .timeout_global(Some(Duration::from_secs(15)))
        .max_redirects(0)
        .http_status_as_error(false)
        .build()
        .into();
    for path in ["audio/voices", "voices"] {
        let mut url = reqwest::Url::parse(&crate::ai::endpoint(&s.tts_url, path)?)?;
        if !s.tts_model.trim().is_empty() {
            url.query_pairs_mut()
                .append_pair("model", s.tts_model.trim());
        }
        let mut req = agent.get(url.as_str());
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
            return Ok((voices, String::new()));
        }
    }
    Ok((
        vec![],
        if s.tts_model.trim().is_empty() {
            "Choose a TTS model to list its voices.".into()
        } else {
            "No voices were returned for this model. You can enter a voice ID manually.".into()
        },
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
        "Select a TTS model and voice in Models and AI"
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
pub fn speak(
    s: &Settings,
    text: &str,
    cancel: &Arc<AtomicBool>,
    paused: &Arc<AtomicBool>,
) -> Result<()> {
    speak_report(s, text, cancel, paused, |_| {})
}
pub fn speak_report(
    s: &Settings,
    text: &str,
    cancel: &Arc<AtomicBool>,
    paused: &Arc<AtomicBool>,
    mut report: impl FnMut(ReceiveStats),
) -> Result<()> {
    use std::io::Write;
    let parts = speech_parts(text);
    anyhow::ensure!(!parts.is_empty(), "No answer to read aloud");
    for part in parts {
        if cancel.load(Ordering::Acquire) {
            break;
        }
        let mut input: Option<std::process::ChildStdin> = None;
        let mut format = None;
        let received = receive_audio(s, &part, cancel, |audio, kind| {
            while paused.load(Ordering::Acquire) && !cancel.load(Ordering::Acquire) {
                std::thread::sleep(Duration::from_millis(25));
            }
            anyhow::ensure!(!cancel.load(Ordering::Acquire), "Read aloud stopped");
            if input.is_none() {
                input = Some(crate::platform::play_audio_stream(kind.pcm())?);
                format = Some(kind);
                if paused.load(Ordering::Acquire) {
                    crate::platform::pause_playback(true)?;
                }
            }
            anyhow::ensure!(format == Some(kind), "TTS stream changed its audio format");
            input.as_mut().unwrap().write_all(audio)?;
            Ok(())
        });
        drop(input);
        match received {
            Ok(stats) => report(stats),
            Err(e) => {
                crate::platform::stop_playback();
                return Err(e);
            }
        }
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
            std::thread::sleep(Duration::from_millis(50));
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

#[cfg(test)]
mod model_voice_tests {
    use super::*;
    #[test]
    fn audio_cpp_multiple_models_have_separate_voice_lists() {
        let server = tiny_http::Server::http("127.0.0.1:0").unwrap();
        let url = format!("http://{}/v1", server.server_addr());
        let worker = std::thread::spawn(move || {
            for (expected, body) in [
                (
                    "/v1/audio/voices?model=pocket-tts",
                    r#"{"voices":["alba","cosette"]}"#,
                ),
                (
                    "/v1/audio/voices?model=kokoro%2F8bit",
                    r#"{"voices":["af_heart"]}"#,
                ),
            ] {
                let req = server.recv().unwrap();
                assert_eq!(req.url(), expected);
                req.respond(tiny_http::Response::from_string(body)).unwrap();
            }
        });
        let mut s = Settings {
            tts_url: url,
            tts_model: "pocket-tts".into(),
            ..Default::default()
        };
        assert_eq!(voices(&s).unwrap().0, vec!["alba", "cosette"]);
        s.tts_model = "kokoro/8bit".into();
        assert_eq!(voices(&s).unwrap().0, vec!["af_heart"]);
        worker.join().unwrap();
    }
}

#[cfg(test)]
mod catalog_model_error_tests {
    use super::*;
    #[test]
    fn model_list_survives_an_invalid_saved_voice_model() {
        let server = tiny_http::Server::http("127.0.0.1:0").unwrap();
        let s = Settings {
            tts_url: format!("http://{}/v1", server.server_addr()),
            tts_model: "missing".into(),
            ..Default::default()
        };
        let worker = std::thread::spawn(move || {
            let req = server.recv().unwrap();
            assert_eq!(req.url(), "/v1/models");
            req.respond(tiny_http::Response::from_string(
                r#"{"data":[{"id":"pocket-tts"},{"id":"kokoro"}]}"#,
            ))
            .unwrap();
            let req = server.recv().unwrap();
            assert_eq!(req.url(), "/v1/audio/voices?model=missing");
            req.respond(tiny_http::Response::from_string("{}").with_status_code(400))
                .unwrap();
        });
        let (models, voices, notice) = catalog(&s).unwrap();
        assert_eq!(models, vec!["kokoro", "pocket-tts"]);
        assert!(voices.is_empty());
        assert!(notice.contains("400"));
        worker.join().unwrap();
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum AudioKind {
    Encoded,
    Pcm(u32, u16),
}
impl AudioKind {
    fn pcm(self) -> Option<(u32, u16)> {
        match self {
            Self::Encoded => None,
            Self::Pcm(rate, channels) => Some((rate, channels)),
        }
    }
}
#[derive(Clone, Debug, serde::Serialize)]
pub struct ReceiveStats {
    pub route: String,
    pub first_audio_ms: f64,
    pub total_ms: f64,
    pub bytes: usize,
}
#[derive(Clone, Copy, PartialEq)]
enum Route {
    Sse,
    Mp3Stream,
    Wav,
    Mp3,
}
impl Route {
    fn name(self) -> &'static str {
        match self {
            Self::Sse => "PCM SSE streaming",
            Self::Mp3Stream => "MP3 streaming",
            Self::Wav => "WAV audio transport",
            Self::Mp3 => "MP3 audio transport",
        }
    }
    fn body(self, s: &Settings, text: &str) -> Value {
        let mut v = json!({"model":s.tts_model,"voice":s.tts_voice,"input":text,"response_format":if self==Self::Sse{"pcm"}else if self==Self::Wav{"wav"}else{"mp3"}});
        if self == Self::Sse {
            v["stream_format"] = json!("sse");
        } else if self == Self::Mp3Stream {
            v["stream_format"] = json!("audio");
        }
        v
    }
}
static ROUTES: std::sync::OnceLock<
    std::sync::Mutex<std::collections::HashMap<String, (Route, std::time::Instant)>>,
> = std::sync::OnceLock::new();
pub fn reset_stream_cache() {
    if let Some(c) = ROUTES.get() {
        if let Ok(mut c) = c.lock() {
            c.clear();
        }
    }
}
fn unsupported(status: u16, message: &str) -> bool {
    let m = message.to_lowercase();
    matches!(status, 404 | 405 | 415 | 501)
        || matches!(status, 400 | 422)
            && (m.contains("stream") || m.contains("format") || m.contains("unsupported"))
}
#[derive(Default)]
struct SseAudio {
    pending: Vec<u8>,
    data: String,
}
impl SseAudio {
    fn push(&mut self, bytes: &[u8]) -> Result<Vec<Value>> {
        self.pending.extend_from_slice(bytes);
        anyhow::ensure!(
            self.pending.len() <= 2 * 1024 * 1024,
            "TTS stream event exceeded the limit"
        );
        let mut events = vec![];
        while let Some(end) = self.pending.iter().position(|b| *b == b'\n') {
            let line = String::from_utf8(self.pending.drain(..=end).collect())?;
            let line = line.trim_end_matches(['\r', '\n']);
            if line.is_empty() {
                if !self.data.is_empty() {
                    if self.data.trim() == "[DONE]" {
                        events.push(json!({"type":"done"}));
                    } else {
                        events.push(serde_json::from_str(&self.data)?);
                    }
                    self.data.clear();
                }
            } else if let Some(data) = line.strip_prefix("data:") {
                if !self.data.is_empty() {
                    self.data.push('\n');
                }
                self.data.push_str(data.trim_start());
                anyhow::ensure!(
                    self.data.len() <= 2 * 1024 * 1024,
                    "TTS stream event exceeded the limit"
                );
            }
        }
        Ok(events)
    }
}
pub fn receive_audio(
    s: &Settings,
    text: &str,
    cancel: &Arc<AtomicBool>,
    mut sink: impl FnMut(&[u8], AudioKind) -> Result<()>,
) -> Result<ReceiveStats> {
    use base64::Engine;
    anyhow::ensure!(
        !s.tts_model.trim().is_empty() && !s.tts_voice.trim().is_empty(),
        "Select a TTS model and voice in Models and AI"
    );
    let url = crate::ai::endpoint(&s.tts_url, "audio/speech")?;
    let key = format!("{}|{}", s.tts_url, s.tts_model);
    let cached = ROUTES
        .get_or_init(|| std::sync::Mutex::new(Default::default()))
        .lock()
        .unwrap()
        .get(&key)
        .filter(|(_, at)| at.elapsed() < Duration::from_secs(60))
        .map(|(route, _)| *route);
    let mut routes = vec![Route::Sse, Route::Mp3Stream, Route::Wav, Route::Mp3];
    if let Some(cached) = cached {
        routes.retain(|r| *r != cached);
        routes.insert(0, cached);
    }
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()?;
    let start = std::time::Instant::now();
    runtime.block_on(async {
  let client=reqwest::Client::builder().connect_timeout(Duration::from_secs(5)).timeout(Duration::from_secs(120)).redirect(reqwest::redirect::Policy::none()).build()?;
  let cancellation=async{while !cancel.load(Ordering::Acquire){tokio::time::sleep(Duration::from_millis(25)).await;}};tokio::pin!(cancellation);
  let mut last_error="TTS server does not support the requested audio formats".to_owned();
  'route: for route in routes {
   let mut req=client.post(&url).json(&route.body(s,text));if !s.tts_key.trim().is_empty(){req=req.bearer_auth(s.tts_key.trim());}if route==Route::Sse{req=req.header("Accept","text/event-stream");}
   let mut response=tokio::select!{biased;_=&mut cancellation=>anyhow::bail!("Read aloud stopped"),r=req.send()=>r.context("Could not connect to the TTS server")?};
   let status=response.status().as_u16();let content=response.headers().get("content-type").and_then(|h|h.to_str().ok()).unwrap_or("").to_lowercase();
   if !response.status().is_success(){let mut bytes=vec![];loop{let chunk=tokio::select!{biased;_=&mut cancellation=>anyhow::bail!("Read aloud stopped"),r=response.chunk()=>r?};let Some(chunk)=chunk else{break};let take=(8192-bytes.len()).min(chunk.len());bytes.extend_from_slice(&chunk[..take]);if bytes.len()>=8192{break;}}
    let value:Value=serde_json::from_slice(&bytes).unwrap_or(Value::Null);let message=value["error"]["message"].as_str().unwrap_or("");last_error=format!("TTS server HTTP {status}: {}",message.chars().take(300).collect::<String>());if unsupported(status,message){continue;}anyhow::bail!("{last_error}");
   }
   let sse=content.contains("text/event-stream");anyhow::ensure!(sse||content.starts_with("audio/")||content.contains("octet-stream"),"TTS server did not return audio");
   let mut decoder=SseAudio::default();let mut bytes=0usize;let mut wire=0usize;let mut first=None;let mut done=false;
   loop {
    let chunk=tokio::select!{biased;_=&mut cancellation=>anyhow::bail!("Read aloud stopped"),r=response.chunk()=>r?};let Some(chunk)=chunk else{break};wire+=chunk.len();anyhow::ensure!(wire<=48*1024*1024,"TTS stream exceeded the limit");
    if sse {
     for event in decoder.push(&chunk)? {
      if event["type"]=="error" {let message=event["error"]["message"].as_str().unwrap_or("Speech generation failed");if bytes==0&&unsupported(400,message){last_error=message.chars().take(300).collect();continue 'route;}anyhow::bail!("TTS server: {}",message.chars().take(300).collect::<String>());}
      if event["type"]=="speech.audio.delta" {let encoded=event["audio"].as_str().or_else(||event["delta"].as_str()).context("Missing streamed audio")?;let pcm=base64::engine::general_purpose::STANDARD.decode(encoded)?;bytes+=pcm.len();anyhow::ensure!(bytes<=32*1024*1024,"TTS audio exceeded the limit");if !pcm.is_empty(){let rate=event["sample_rate"].as_u64().unwrap_or(24000);let channels=event["channels"].as_u64().unwrap_or(1);anyhow::ensure!((8000..=192000).contains(&rate)&&(1..=8).contains(&channels),"Invalid streamed PCM format");if first.is_none(){first=Some(start.elapsed().as_secs_f64()*1000.0);}sink(&pcm,AudioKind::Pcm(rate as u32,channels as u16))?;}}
      if event["type"]=="speech.audio.done"||event["type"]=="done"{done=true;}
     }
     if done{break;}
    }else {bytes+=chunk.len();anyhow::ensure!(bytes<=32*1024*1024,"TTS audio exceeded the limit");if !chunk.is_empty(){if first.is_none(){first=Some(start.elapsed().as_secs_f64()*1000.0);}let kind=if content.contains("pcm"){AudioKind::Pcm(24000,1)}else{AudioKind::Encoded};sink(&chunk,kind)?;}}
   }
   anyhow::ensure!(bytes>0,"TTS server returned no audio");if sse{anyhow::ensure!(done,"TTS stream ended before speech.audio.done");}
   let mut cache=ROUTES.get().unwrap().lock().unwrap();if cache.len()>=16{cache.clear();}cache.insert(key.clone(),(route,std::time::Instant::now()));
   return Ok(ReceiveStats{route:route.name().into(),first_audio_ms:first.unwrap_or_default(),total_ms:start.elapsed().as_secs_f64()*1000.0,bytes});
  }
  anyhow::bail!("{last_error}")
 })
}

#[cfg(test)]
mod stream_tests {
    use super::*;
    #[test]
    fn fragmented_sse_and_case_sensitive_ids() {
        let mut parser = SseAudio::default();
        let data = b"event: speech.audio.delta\r\ndata: {\"type\":\"speech.audio.delta\",\"audio\":\"AAAA\"}\r\n\r\ndata: [DONE]\n\n";
        let mut events = vec![];
        for byte in data {
            events.extend(parser.push(&[*byte]).unwrap());
        }
        assert_eq!(events.len(), 2);
        assert_eq!(events[0]["audio"], "AAAA");
        assert_eq!(events[1]["type"], "done");
        let settings = Settings {
            tts_model: "omnivoice".into(),
            tts_voice: "idris".into(),
            ..Default::default()
        };
        assert_eq!(Route::Sse.body(&settings, "test")["model"], "omnivoice");
        assert_eq!(Route::Sse.body(&settings, "test")["voice"], "idris");
        assert!(!unsupported(401, "unsupported format"));
        assert!(!unsupported(429, "stream limit"));
        assert!(unsupported(400, "unsupported response_format=pcm"));
    }
}
