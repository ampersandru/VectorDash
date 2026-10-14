use anyhow::{Context, Result};
use serde_json::{Value, json};
use std::{
    path::Path,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    time::Duration,
};
#[derive(Clone, Debug)]
pub struct Catalog {
    pub models: Vec<String>,
    pub note: String,
    pub context: Option<u32>,
    pub vision: Option<bool>,
    pub dimensions: Option<usize>,
}
pub fn api(base: &str, path: &str) -> Result<String> {
    let base = base.trim().trim_end_matches('/');
    crate::ai::endpoint(
        &format!("{}/v1", base.strip_suffix("/v1").unwrap_or(base)),
        path,
    )
}
fn client() -> Result<reqwest::Client> {
    Ok(reqwest::Client::builder()
        .connect_timeout(Duration::from_secs(5))
        .timeout(Duration::from_secs(120))
        .redirect(reqwest::redirect::Policy::none())
        .build()?)
}
async fn json_response(response: reqwest::Response) -> Result<Value> {
    let status = response.status();
    let mut response = response;
    let mut bytes = vec![];
    while let Some(chunk) = response.chunk().await? {
        anyhow::ensure!(
            bytes.len() + chunk.len() <= 1024 * 1024,
            "Server response exceeded the limit"
        );
        bytes.extend_from_slice(&chunk);
    }
    let value: Value = serde_json::from_slice(&bytes).context("Server did not return JSON")?;
    anyhow::ensure!(
        status.is_success(),
        "Server HTTP {}: {}",
        status.as_u16(),
        value["error"]["message"]
            .as_str()
            .unwrap_or("Request failed")
            .chars()
            .take(300)
            .collect::<String>()
    );
    Ok(value)
}
pub fn context_limit(value: &Value) -> Option<u32> {
    if let Some(args) = value.pointer("/status/args").and_then(Value::as_array) {
        for (index, arg) in args.iter().enumerate() {
            if let Some(arg) = arg.as_str() {
                if ["--ctx-size", "-c", "-ctx"].contains(&arg) {
                    if let Some(n) = args
                        .get(index + 1)
                        .and_then(Value::as_str)
                        .and_then(|v| v.parse::<u32>().ok())
                        .filter(|n| *n >= 128)
                    {
                        return Some(n);
                    }
                }
                if let Some(v) = arg
                    .strip_prefix("--ctx-size=")
                    .and_then(|v| v.parse::<u32>().ok())
                    .filter(|n| *n >= 128)
                {
                    return Some(v);
                }
            }
        }
    }

    for key in [
        "n_ctx",
        "context_length",
        "max_model_len",
        "max_context_length",
        "context_window",
        "max_context",
    ] {
        if let Some(n) = value
            .get(key)
            .and_then(Value::as_u64)
            .filter(|n| *n >= 128 && *n <= u32::MAX as u64)
        {
            return Some(n as u32);
        }
    }
    for key in [
        "default_generation_settings",
        "meta",
        "metadata",
        "config",
        "model_info",
    ] {
        if let Some(n) = value.get(key).and_then(context_limit) {
            return Some(n);
        }
    }
    None
}
fn vision_support(value: &Value) -> Option<bool> {
    if let Some(inputs) = value
        .pointer("/architecture/input_modalities")
        .and_then(Value::as_array)
    {
        return Some(inputs.iter().any(|v| v == "image" || v == "video"));
    }

    value
        .pointer("/modalities/vision")
        .and_then(Value::as_bool)
        .or_else(|| {
            value
                .get("mmproj")
                .and_then(|v| v.as_bool().or_else(|| v.as_str().map(|s| !s.is_empty())))
        })
        .or_else(|| {
            value
                .pointer("/capabilities/vision")
                .and_then(Value::as_bool)
        })
}
pub fn catalog(base: &str, key: &str, model: &str, check_embeddings: bool) -> Result<Catalog> {
    catalog_inner(base, key, model, check_embeddings, true)
}
pub fn service_catalog(
    base: &str,
    key: &str,
    model: &str,
    embedding: bool,
    contextual: bool,
) -> Result<Catalog> {
    catalog_inner(base, key, model, embedding, contextual)
}
fn catalog_inner(
    base: &str,
    key: &str,
    model: &str,
    check_embeddings: bool,
    contextual: bool,
) -> Result<Catalog> {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()?;
    runtime.block_on(async {
        let client = client()?;
        let mut req = client.get(api(base, "models")?);
        if !key.trim().is_empty() {
            req = req.bearer_auth(key.trim());
        }
        let value = json_response(req.send().await?).await?;
        let mut models = value["data"]
            .as_array()
            .context("Server did not return a model list")?
            .iter()
            .filter_map(|v| v["id"].as_str())
            .take(1000)
            .map(str::to_owned)
            .collect::<Vec<_>>();
        models.sort();
        models.dedup();
        let mut dimensions = None;
        let mut note = if check_embeddings {
            anyhow::ensure!(!model.trim().is_empty(), "Select an embedding model first");
            let mut req = client
                .post(api(base, "embeddings")?)
                .json(&json!({"model":model,"input":"VectorDash embedding connection test."}));
            if !key.trim().is_empty() {
                req = req.bearer_auth(key.trim());
            }
            let value = json_response(req.send().await?).await?;
            let vector = value["data"][0]["embedding"]
                .as_array()
                .context("Embeddings are not enabled: no vector returned")?;
            anyhow::ensure!(
                !vector.is_empty()
                    && vector
                        .iter()
                        .all(|v| v.as_f64().is_some_and(f64::is_finite)),
                "Invalid embedding vector"
            );
            dimensions = Some(vector.len());
            format!("Embeddings enabled · {} dimensions", vector.len())
        } else {
            "Connection Successful".into()
        };
        let selected = value["data"].as_array().and_then(|a| {
            a.iter()
                .find(|v| v["id"].as_str() == Some(model))
                .or_else(|| if a.len() == 1 { a.first() } else { None })
        });
        anyhow::ensure!(selected.is_some() || model.is_empty(),"Selected model is not listed by the remote server; refresh models and select its exact ID");
        let mut context = if contextual{selected.and_then(context_limit)}else{None};
        let mut vision = selected.and_then(vision_support);
        let root = base
            .trim()
            .trim_end_matches('/')
            .strip_suffix("/v1")
            .unwrap_or(base.trim().trim_end_matches('/'));
        let mut url = reqwest::Url::parse(&crate::ai::endpoint(root, "props")?)?;
        if !model.is_empty() {
            url.query_pairs_mut().append_pair("model", model).append_pair("autoload","false");
        }
        let mut props = client.get(url).timeout(Duration::from_secs(5));
        if !key.trim().is_empty() {
            props = props.bearer_auth(key.trim());
        }
        if contextual { if let Ok(response) = props.send().await {
            if let Ok(props) = json_response(response).await {
                context = context_limit(&props).or(context);
                vision = vision_support(&props).or(vision);
            }
        }
        }
        if check_embeddings {
            note.push_str(match vision {
                Some(true) => " · mmproj detected (vision ready)",
                Some(false) => " · mmproj not detected (vision unavailable)",
                None => " · mmproj not detected: server did not report vision support",
            });
            if dimensions.is_some_and(|d| d < 768) {
                note.push_str(
                    " · Warning: fewer than 768 dimensions; retrieval quality may be reduced",
                );
            }
        }
        if let Some(context) = context {
            note.push_str(&format!(" · Context {context} tokens"));
        }
        Ok(Catalog {
            models,
            note,
            context,
            vision,
            dimensions,
        })
    })
}
pub fn transcribe(
    s: &crate::config::Settings,
    wav: &Path,
    cancel: &Arc<AtomicBool>,
) -> Result<String> {
    anyhow::ensure!(
        !s.dictation_model.trim().is_empty(),
        "Select a remote dictation model in Models and AI"
    );
    let audio = std::fs::read(wav)?;
    anyhow::ensure!(
        audio.len() <= 4 * 1024 * 1024,
        "Dictation audio exceeded the limit"
    );
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()?;
    runtime.block_on(async {
        let part=reqwest::multipart::Part::bytes(audio).file_name("dictation.wav").mime_str("audio/wav")?;
        let form=reqwest::multipart::Form::new().text("model",s.dictation_model.clone()).text("response_format","json").part("file",part);
        let mut req=client()?.post(api(&s.dictation_url,"audio/transcriptions")?).multipart(form);
        if !s.dictation_key.trim().is_empty(){req=req.bearer_auth(s.dictation_key.trim());}
        let request=async {let value=json_response(req.send().await?).await?;let text=value["text"].as_str().context("Transcription response has no text")?.trim().to_owned();anyhow::ensure!(!text.is_empty(),"Remote dictation returned no recognizable speech");Ok(text)};
        let cancellation=async{while !cancel.load(Ordering::Acquire){tokio::time::sleep(Duration::from_millis(25)).await;}};
        tokio::select!{biased;_ = cancellation => anyhow::bail!("Dictation cancelled"), result=request => result}
    })
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn normalized_api() {
        assert_eq!(
            api("https://example.test/v1/", "models").unwrap(),
            "https://example.test/v1/models"
        );
        assert_eq!(
            api("http://localhost:8080", "embeddings").unwrap(),
            "http://localhost:8080/v1/embeddings"
        );
        assert!(api("http://secret@host/v1", "models").is_err());
    }
}

#[cfg(test)]
mod request_tests {
    use super::*;
    #[test]
    fn embedding_probe_uses_selected_model_and_auth() {
        let server = tiny_http::Server::http("127.0.0.1:0").unwrap();
        let url = format!("http://{}/v1", server.server_addr());
        let task = std::thread::spawn(move || {
            for n in 0..2 {
                let mut req = server.recv().unwrap();
                assert!(req.headers().iter().any(
                    |h| h.field.equiv("Authorization") && h.value.as_str() == "Bearer fixture"
                ));
                let response = if n == 0 {
                    assert_eq!(req.url(), "/v1/models");
                    json!({"data":[{"id":"EmbedCase"}]})
                } else {
                    assert_eq!(req.url(), "/v1/embeddings");
                    let mut body = String::new();
                    req.as_reader().read_to_string(&mut body).unwrap();
                    assert_eq!(
                        serde_json::from_str::<Value>(&body).unwrap()["model"],
                        "EmbedCase"
                    );
                    json!({"data":[{"embedding":[0.2,0.3]}]})
                };
                req.respond(
                    tiny_http::Response::from_string(response.to_string()).with_header(
                        tiny_http::Header::from_bytes("Content-Type", "application/json").unwrap(),
                    ),
                )
                .unwrap();
            }
        });
        let result = catalog(&url, "fixture", "EmbedCase", true).unwrap();
        assert_eq!(result.models, vec!["EmbedCase"]);
        assert!(result.note.contains("2 dimensions"));
        task.join().unwrap();
    }
    #[test]
    fn remote_transcription_uploads_wav_and_keeps_model_case() {
        let server = tiny_http::Server::http("127.0.0.1:0").unwrap();
        let settings = crate::config::Settings {
            dictation_server: "Remote".into(),
            dictation_url: format!("http://{}/v1", server.server_addr()),
            dictation_key: "asr-key".into(),
            dictation_model: "ParakeetCase".into(),
            ..Default::default()
        };
        let task = std::thread::spawn(move || {
            let mut req = server.recv().unwrap();
            assert_eq!(req.url(), "/v1/audio/transcriptions");
            assert!(req.headers().iter().any(|h|h.field.equiv("Authorization")&&h.value.as_str()=="Bearer asr-key"));
            let mut body = vec![];
            req.as_reader().read_to_end(&mut body).unwrap();
            let body = String::from_utf8_lossy(&body);
            assert!(body.contains("name=\"model\""));
            assert!(body.contains("ParakeetCase"));
            assert!(body.contains("filename=\"dictation.wav\""));
            assert!(body.contains("RIFF"));
            req.respond(
                tiny_http::Response::from_string("{\"text\":\"Hello, world.\"}").with_header(
                    tiny_http::Header::from_bytes("Content-Type", "application/json").unwrap(),
                ),
            )
            .unwrap();
        });
        let temp = tempfile::tempdir().unwrap();
        let wav = temp.path().join("test.wav");
        std::fs::write(&wav, b"RIFFtestWAVE").unwrap();
        assert_eq!(
            transcribe(&settings, &wav, &Arc::new(AtomicBool::new(false))).unwrap(),
            "Hello, world."
        );
        task.join().unwrap();
    }
    #[test]
    fn cancelled_transcription_does_not_wait_for_server() {
        let s = crate::config::Settings {
            dictation_url: "http://127.0.0.1:9/v1".into(),
            dictation_model: "model".into(),
            ..Default::default()
        };
        let temp = tempfile::tempdir().unwrap();
        let wav = temp.path().join("test.wav");
        std::fs::write(&wav, b"RIFFtest").unwrap();
        let start = std::time::Instant::now();
        let error = transcribe(&s, &wav, &Arc::new(AtomicBool::new(true))).unwrap_err();
        assert!(error.to_string().contains("cancelled"));
        assert!(start.elapsed() < Duration::from_secs(1));
    }
}

#[cfg(test)]
mod capability_tests {
    use super::*;
    #[test]
    fn runtime_context_and_explicit_vision_are_detected() {
        assert_eq!(
            context_limit(&json!({"default_generation_settings":{"n_ctx":32768}})),
            Some(32768)
        );
        assert_eq!(
            context_limit(&json!({"meta":{"n_ctx_train":8192},"max_context_length":4096})),
            Some(4096)
        );
        assert_eq!(
            vision_support(&json!({"modalities":{"vision":true}})),
            Some(true)
        );
        assert_eq!(
            vision_support(
                &json!({"modalities":{"vision":false},"media_marker":"exists even without projector"})
            ),
            Some(false)
        );
        assert_eq!(vision_support(&json!({"media_marker":"not proof"})), None);
    }
    #[test]
    fn probe_reports_projector_context_and_low_dimensions() {
        for vision in [true, false] {
            let server = tiny_http::Server::http("127.0.0.1:0").unwrap();
            let url = format!("http://{}/v1", server.server_addr());
            let task = std::thread::spawn(move || {
                for n in 0..3 {
                    let mut req = server.recv().unwrap();
                    let value = match n {
                        0 => json!({"data":[{"id":"embedding","max_context_length":8192}]}),
                        1 => json!({"data":[{"embedding":[0.1,0.2]}]}),
                        _ => {
                            assert_eq!(req.url(), "/props?model=embedding&autoload=false");
                            json!({"default_generation_settings":{"n_ctx":2048},"modalities":{"vision":vision}})
                        }
                    };
                    req.respond(
                        tiny_http::Response::from_string(value.to_string()).with_header(
                            tiny_http::Header::from_bytes("Content-Type", "application/json")
                                .unwrap(),
                        ),
                    )
                    .unwrap();
                }
            });
            let result = catalog(&url, "", "embedding", true).unwrap();
            assert_eq!(result.context, Some(2048));
            assert_eq!(result.vision, Some(vision));
            assert!(result.note.contains("Warning"));
            assert!(result.note.contains(if vision {
                "mmproj detected"
            } else {
                "mmproj not detected"
            }));
            task.join().unwrap();
        }
    }
}
