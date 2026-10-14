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
pub fn endpoint(base: &str, path: &str) -> Result<String> {
    let b = base.trim().trim_end_matches('/');
    anyhow::ensure!(
        (b.starts_with("http://") || b.starts_with("https://"))
            && !b.contains(['?', '#', '@'])
            && !b.chars().any(char::is_whitespace),
        "Use an HTTP(S) base URL without credentials/query, for example http://localhost:8080/v1"
    );
    anyhow::ensure!(
        !b.split("://")
            .nth(1)
            .unwrap_or("")
            .split('/')
            .next()
            .unwrap_or("")
            .is_empty(),
        "Missing server host"
    );
    Ok(format!("{b}/{path}"))
}
fn agent() -> ureq::Agent {
    ureq::Agent::config_builder()
        .timeout_connect(Some(Duration::from_secs(5)))
        .timeout_global(Some(Duration::from_secs(120)))
        .max_redirects(0)
        .http_status_as_error(false)
        .build()
        .into()
}
pub fn models(s: &Settings) -> Result<Vec<String>> {
    let agent = agent();
    let mut req = agent.get(endpoint(&s.llm_url, "models")?);
    if !s.llm_key.trim().is_empty() {
        req = req.header("Authorization", format!("Bearer {}", s.llm_key.trim()));
    }
    let mut response = req.call().context("Could not connect to the LLM server")?;
    anyhow::ensure!(
        response.status().is_success(),
        "LLM server returned HTTP {}",
        response.status()
    );
    let data: Value = response
        .body_mut()
        .with_config()
        .limit(1024 * 1024)
        .read_json()?;
    let mut ids = data["data"]
        .as_array()
        .context("Server did not return a model list")?
        .iter()
        .filter_map(|v| v["id"].as_str())
        .take(1000)
        .map(str::to_string)
        .collect::<Vec<_>>();
    ids.sort();
    ids.dedup();
    Ok(ids)
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct Message {
    pub role: String,
    pub content: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub images: Vec<String>,
}
fn estimate(messages: &[Message]) -> usize {
    messages
        .iter()
        .map(|m| m.content.len().div_ceil(3) + 24 + m.images.len() * 2048)
        .sum::<usize>()
        + 32
}
pub fn prepare(s: &Settings, history: &[Message], prompt: &str) -> Result<Vec<Message>> {
    anyhow::ensure!(
        !s.llm_model.trim().is_empty(),
        "Select an LLM model in AI settings"
    );
    anyhow::ensure!(!prompt.trim().is_empty(), "Enter a question first");
    anyhow::ensure!(
        prompt.len() <= 128 * 1024,
        "Question exceeds the text limit"
    );
    let context = s.llm_context.clamp(256, 1048576) as usize;
    let output = (context / 4).min(2048);
    let mut messages = history.to_vec();
    messages.push(Message {
        role: "user".into(),
        content: prompt.into(),
        images: vec![],
    });
    while messages.len() > 1
        && (estimate(&messages) + output > context
            || messages.iter().map(|m| m.content.len()).sum::<usize>() > 128 * 1024
            || messages
                .iter()
                .flat_map(|m| m.images.iter())
                .map(String::len)
                .sum::<usize>()
                > 16 * 1024 * 1024
            || messages.len() > 41)
    {
        messages.drain(..2.min(messages.len() - 1));
    }
    anyhow::ensure!(
        estimate(&messages) + output <= context,
        "Question exceeds the context budget. Increase Max context or shorten it."
    );
    Ok(messages)
}
pub fn wire_messages(messages: &[Message]) -> Result<Vec<Value>> {
    messages
        .iter()
        .map(|m| {
            anyhow::ensure!(m.images.len() <= 1, "Only one screenshot per question");
            if m.images.is_empty() {
                return Ok(json!({"role":m.role,"content":m.content}));
            }
            let mut parts = vec![json!({"type":"text","text":m.content})];
            for image in &m.images {
                anyhow::ensure!(
                    image.starts_with("data:image/jpeg;base64,") && image.len() <= 3 * 1024 * 1024,
                    "Invalid screenshot attachment"
                );
                parts.push(json!({"type":"image_url","image_url":{"url":image}}));
            }
            Ok(json!({"role":m.role,"content":parts}))
        })
        .collect()
}
pub fn attach(s: &Settings, messages: &mut Vec<Message>, images: Vec<String>) -> Result<()> {
    if images.is_empty() {
        return Ok(());
    }
    anyhow::ensure!(
        s.llm_vision,
        "Enable vision for a compatible model in AI settings first"
    );
    let last = messages.last_mut().context("Missing question")?;
    last.images = images;
    wire_messages(messages)?;
    let context = s.llm_context.clamp(256, 1048576) as usize;
    while messages.len() > 1
        && (estimate(messages) + (context / 4).min(2048) > context
            || messages
                .iter()
                .flat_map(|m| m.images.iter())
                .map(String::len)
                .sum::<usize>()
                > 16 * 1024 * 1024)
    {
        messages.drain(..2.min(messages.len() - 1));
    }
    anyhow::ensure!(
        estimate(messages) + (context / 4).min(2048) <= context,
        "Increase Max context to fit the screenshot"
    );
    Ok(())
}
#[cfg(test)]
pub fn request_body(s: &Settings, prompt: &str) -> Result<Value> {
    body(s, &prepare(s, &[], prompt)?)
}
pub fn body(s: &Settings, messages: &[Message]) -> Result<Value> {
    anyhow::ensure!(s.llm_enabled, "AI chat is disabled in settings");
    anyhow::ensure!(
        !s.llm_model.trim().is_empty(),
        "Select an LLM model in AI settings"
    );
    let context = s.llm_context.clamp(256, 1048576) as usize;
    let output = (context / 4).min(2048);
    anyhow::ensure!(
        estimate(messages) + output <= context,
        "Conversation exceeds the context budget"
    );
    Ok(
        json!({"model":s.llm_model,"messages":wire_messages(messages)?,"max_tokens":output,"stream":true}),
    )
}
pub fn ask(
    s: &Settings,
    messages: &[Message],
    cancel: &Arc<AtomicBool>,
    mut chunk: impl FnMut(String),
) -> Result<()> {
    let body = body(s, messages)?;
    let url = endpoint(&s.llm_url, "chat/completions")?;
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()?;
    runtime.block_on(async {
        let client=reqwest::Client::builder().connect_timeout(Duration::from_secs(5)).timeout(Duration::from_secs(120)).redirect(reqwest::redirect::Policy::none()).build()?;
        let mut req=client.post(url).json(&body);
        if !s.llm_key.trim().is_empty(){req=req.bearer_auth(s.llm_key.trim());}
        let cancellation=async {while !cancel.load(Ordering::Acquire){tokio::time::sleep(Duration::from_millis(25)).await;}};
        tokio::pin!(cancellation);
        let mut response=tokio::select! { biased; _=&mut cancellation=>return Ok(()), result=req.send()=>result.context("Could not connect to the LLM server")? };
        anyhow::ensure!(response.status().is_success(),"LLM server returned HTTP {}",response.status());
        let json_response=response.headers().get("content-type").and_then(|v|v.to_str().ok()).is_some_and(|v|v.contains("application/json"));
        let mut pending=Vec::new();let mut total=0usize;let mut wire=0usize;let mut received=false;
        loop {
            let bytes=tokio::select! {biased; _=&mut cancellation=>return Ok(()), result=response.chunk()=>result?};
            let Some(bytes)=bytes else {break};
            wire+=bytes.len();anyhow::ensure!(wire<=4*1024*1024,"Server response exceeded the limit");
            pending.extend_from_slice(&bytes);
            if json_response {continue}
            while let Some(end)=pending.iter().position(|b|*b==b'\n') {
                let line=String::from_utf8(pending.drain(..=end).collect())?;
                if let Some(data)=line.strip_prefix("data:").map(str::trim) {
                    if data=="[DONE]"{anyhow::ensure!(received,"Server returned no answer");return Ok(())}
                    let value:Value=serde_json::from_str(data)?;
                    anyhow::ensure!(value.get("error").is_none(),"LLM server reported an error");
                    if let Some(text)=value["choices"][0]["delta"]["content"].as_str() {
                        total+=text.len();anyhow::ensure!(total<=128*1024,"Answer exceeded the display limit");
                        if !text.is_empty(){received=true;chunk(text.into());}
                    }
                }
            }
            anyhow::ensure!(pending.len()<=256*1024,"Server stream line exceeded the limit");
        }
        if json_response {
            let v:Value=serde_json::from_slice(&pending)?;
            let text=v["choices"][0]["message"]["content"].as_str().context("Server returned no answer")?;
            anyhow::ensure!(text.len()<=128*1024,"Answer exceeded the display limit");
            chunk(text.into());return Ok(())
        }
        anyhow::ensure!(received,"Server returned no answer");Ok(())
    })
}
pub fn shortcut(key: eframe::egui::Key, m: eframe::egui::Modifiers) -> String {
    format!(
        "{}{}{}{:?}",
        if m.ctrl { "Ctrl+" } else { "" },
        if m.alt { "Alt+" } else { "" },
        if m.shift { "Shift+" } else { "" },
        key
    )
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn endpoints_and_context_are_bounded() {
        assert_eq!(
            endpoint("http://localhost:8080/v1/", "models").unwrap(),
            "http://localhost:8080/v1/models"
        );
        assert!(endpoint("http://key@host/v1", "models").is_err());
        let s = Settings {
            llm_model: "test".into(),
            llm_context: 256,
            ..Default::default()
        };
        assert!(request_body(&s, &"a".repeat(1000)).is_err());
        let body = request_body(&s, "hello").unwrap();
        assert_eq!(body["messages"].as_array().unwrap().len(), 1);
        assert_eq!(body["messages"][0]["content"], "hello");
    }
    #[test]
    fn streamed_answer_and_model_refresh_use_configured_server() {
        let server = tiny_http::Server::http("127.0.0.1:0").unwrap();
        let url = format!("http://{}/v1", server.server_addr());
        let worker = std::thread::spawn(move || {
            let request = server.recv().unwrap();
            assert_eq!(request.url(), "/v1/models");
            request
                .respond(tiny_http::Response::from_string(
                    r#"{"data":[{"id":"local-test"}]}"#,
                ))
                .unwrap();
            let mut request = server.recv().unwrap();
            assert_eq!(request.url(), "/v1/chat/completions");
            let mut body = String::new();
            request.as_reader().read_to_string(&mut body).unwrap();
            let body: Value = serde_json::from_str(&body).unwrap();
            assert_eq!(body["messages"][0]["content"], "hello");
            request.respond(tiny_http::Response::from_string("data: {\"choices\":[{\"delta\":{\"content\":\"Hello \"}}]}\n\ndata: {\"choices\":[{\"delta\":{\"content\":\"world\"}}]}\n\ndata: [DONE]\n\n").with_header(tiny_http::Header::from_bytes("Content-Type","text/event-stream").unwrap())).unwrap();
        });
        let s = Settings {
            llm_url: url,
            llm_model: "local-test".into(),
            ..Default::default()
        };
        assert_eq!(models(&s).unwrap(), vec!["local-test"]);
        let mut answer = String::new();
        ask(
            &s,
            &prepare(&s, &[], "hello").unwrap(),
            &Arc::new(AtomicBool::new(false)),
            |v| answer.push_str(&v),
        )
        .unwrap();
        assert_eq!(answer, "Hello world");
        worker.join().unwrap();
    }
}

#[cfg(test)]
mod conversation_tests {
    use super::*;
    #[test]
    fn followups_keep_prior_answers_and_drop_old_turns_at_budget() {
        let s = Settings {
            llm_model: "test".into(),
            llm_context: 4096,
            ..Default::default()
        };
        let history = vec![
            Message {
                role: "user".into(),
                content: "My dog is named Kaya".into(),
                images: vec![],
            },
            Message {
                role: "assistant".into(),
                content: "Hello Kaya!".into(),
                images: vec![],
            },
        ];
        let messages = prepare(&s, &history, "What is her name?").unwrap();
        assert_eq!(messages.len(), 3);
        assert!(
            body(&s, &messages).unwrap()["messages"][1]["content"]
                .as_str()
                .unwrap()
                .contains("Kaya")
        );
        let small = Settings {
            llm_context: 256,
            ..s
        };
        let huge = vec![
            Message {
                role: "user".into(),
                content: "a".repeat(1000),
                images: vec![],
            },
            Message {
                role: "assistant".into(),
                content: "b".repeat(1000),
                images: vec![],
            },
        ];
        let messages = prepare(&small, &huge, "hello").unwrap();
        assert_eq!(messages.len(), 1);
        assert_eq!(messages[0].content, "hello");
    }
    #[test]
    fn stop_disconnects_without_waiting_for_server_response() {
        let server = tiny_http::Server::http("127.0.0.1:0").unwrap();
        let url = format!("http://{}/v1", server.server_addr());
        let (tx, rx) = std::sync::mpsc::channel();
        let worker = std::thread::spawn(move || {
            let mut request = server.recv().unwrap();
            let mut text = String::new();
            request.as_reader().read_to_string(&mut text).unwrap();
            tx.send(()).unwrap();
            std::thread::sleep(Duration::from_secs(2));
            drop(request);
        });
        let cancel = Arc::new(AtomicBool::new(false));
        let cancel2 = cancel.clone();
        let s = Settings {
            llm_url: url,
            llm_model: "test".into(),
            ..Default::default()
        };
        let client = std::thread::spawn(move || {
            ask(&s, &prepare(&s, &[], "hello").unwrap(), &cancel2, |_| {})
        });
        rx.recv_timeout(Duration::from_secs(5)).unwrap();
        let start = std::time::Instant::now();
        cancel.store(true, Ordering::Release);
        client.join().unwrap().unwrap();
        assert!(start.elapsed() < Duration::from_millis(500));
        worker.join().unwrap();
    }
}

#[cfg(test)]
mod vision_tests {
    use super::*;
    #[test]
    fn vision_requires_opt_in_and_encodes_only_explicit_attachment() {
        let mut s = Settings {
            llm_model: "test".into(),
            llm_context: 8192,
            ..Default::default()
        };
        let mut messages = prepare(&s, &[], "What is in this screenshot?").unwrap();
        let image = "data:image/jpeg;base64,/9j/test".to_string();
        assert!(attach(&s, &mut messages, vec![image.clone()]).is_err());
        assert!(messages[0].images.is_empty());
        s.llm_vision = true;
        attach(&s, &mut messages, vec![image.clone()]).unwrap();
        let body = body(&s, &messages).unwrap();
        assert_eq!(body["messages"][0]["content"][0]["type"], "text");
        assert_eq!(body["messages"][0]["content"][1]["image_url"]["url"], image);
        let follow = prepare(&s, &messages, "Tell me more").unwrap();
        assert_eq!(follow[0].images.len(), 1);
        assert!(follow[1].images.is_empty());
        let mut latest = prepare(&s, &[], "hello").unwrap();
        assert!(
            attach(
                &s,
                &mut latest,
                vec!["https://external.invalid/private.png".into()]
            )
            .is_err()
        );
    }
}

#[cfg(test)]
mod disabled_chat_tests {
    #[test]
    fn disabled_chat_cannot_create_a_request_even_with_a_configured_model() {
        let s = crate::config::Settings {
            llm_enabled: false,
            llm_model: "fixture".into(),
            ..Default::default()
        };
        assert!(
            super::request_body(&s, "hello")
                .unwrap_err()
                .to_string()
                .contains("disabled")
        );
    }
}
