//! Numeric-only diagnostics. Never retain or log prompts, tool arguments, URLs or image data.
use super::Metrics;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::{
    cell::RefCell,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    time::Instant,
};

/// Allowlisted action metadata only: never include arguments, names, paths or error text.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct ActionDiagnostics {
    pub tool: String,
    pub action: String,
    pub outcome: String,
    pub error_category: Option<String>,
    pub method: Option<String>,
    pub elements: usize,
    pub inspection_limited: bool,
    pub scroll_moved: Option<bool>,
    pub scroll_method: Option<String>,
    pub scroll_at_boundary: Option<bool>,
    pub snapshot_bound: bool,
    pub decision_observation_offset: usize,
    pub observation_offset: usize,
    pub decision_probability: Option<f64>,
    pub decision_runner_up: Option<f64>,
    pub decision_margin: Option<f64>,
    pub decision_threshold: Option<f64>,
    pub decision_candidates: usize,
    pub decision_escalation_reason: Option<String>,
    pub decision_cached: bool,
    pub decision_mode: Option<String>,
    pub decision_status: Option<String>,
    pub decision_route: Option<String>,
    pub decision_fallback_reason: Option<String>,
    pub decision_vision: bool,
    pub decision_backend: Option<String>,
    pub decision_runtime_loaded: bool,
    pub decision_vision_images: usize,
    pub decision_inference_ms: Option<f64>,
    pub decision_calls: usize,
    pub decision_executed: usize,
    pub decision_ms: Option<f64>,
}
pub fn error_category(message: &str) -> &'static str {
    let value = message.to_ascii_lowercase();
    if value.contains("user did not allow") {
        "permission_denied"
    } else if value.contains("repeated action")
        || value.contains("six input attempts")
        || value.contains("action budget ended")
    {
        "retry_limit"
    } else if !value.contains("stale")
        && (value.contains("fresh observation")
            || value.contains("observe the target window first")
            || value.contains("take a snapshot before acting"))
    {
        "missing_observation"
    } else if value.contains("unsupported key") {
        "unsupported_key"
    } else if value.contains("foreground")
        || value.contains("focus")
        || value.contains("covered by another")
    {
        "focus_blocked"
    } else if value.contains("stale")
        || value.contains("snapshot")
        || value.contains("since observation")
        || value.contains("inspect the target")
    {
        "stale_observation"
    } else if value.contains("ambiguous")
        || value.contains("unique matching")
        || value.contains("identity")
    {
        "ambiguous_target"
    } else if value.contains("verification")
        || value.contains("acknowledge")
        || value.contains("observations disagree")
    {
        "text_verification"
    } else if value.contains("not found")
        || value.contains("unknown element")
        || value.contains("offscreen")
    {
        "target_unavailable"
    } else if value.contains("password")
        || value.contains("protected")
        || value.contains("security")
    {
        "protected_target"
    } else if value.contains("timed out") || value.contains("timeout") {
        "helper_timeout"
    } else {
        "provider_error"
    }
}
fn probability(value: &Value) -> Option<f64> {
    value
        .as_f64()
        .filter(|v| v.is_finite() && (0.0..=1.0).contains(v))
}
fn decision_label(value: &Value) -> Option<String> {
    value
        .as_str()
        .filter(|v| {
            matches!(
                *v,
                "local"
                    | "remote"
                    | "Off"
                    | "Suggest"
                    | "Execute"
                    | "automatic_plan"
                    | "automatic_fallback"
                    | "cached_escalation"
                    | "model_escalation"
                    | "low_confidence"
                    | "low_margin"
                    | "selected"
                    | "unsupported_target"
                    | "local_plan"
                    | "fallback"
                    | "blocked"
                    | "direct"
                    | "verified"
                    | "suggested"
                    | "vision_unavailable"
                    | "vision_capture_failed"
                    | "target_missing"
                    | "target_ambiguous"
                    | "invalid_step"
                    | "no_candidates"
                    | "unavailable"
                    | "escalate"
                    | "action_failed"
                    | "verification_failed"
                    | "needs_verification"
                    | "plan_failed"
                    | "observation_failed"
                    | "direct_fallback"
                    | "plan_required"
            )
        })
        .map(str::to_owned)
}
pub fn actions(calls: &[Value], replies: &[Value]) -> Vec<ActionDiagnostics> {
    calls
        .iter()
        .filter_map(|call| {
            let tool = call["function"]["name"].as_str()?;
            if !matches!(tool, "computer_use" | "browser_use") {
                return None;
            }
            let args: Value = serde_json::from_str(call["function"]["arguments"].as_str()?).ok()?;
            let action = match args["action"].as_str().unwrap_or("") {
                a @ ("applications" | "launch" | "windows" | "decide" | "focus" | "inspect"
                | "screenshot" | "click" | "type" | "key" | "scroll" | "open" | "navigate"
                | "snapshot" | "back" | "close") => a,
                _ => "other",
            };
            let reply = replies.iter().find(|r| r["tool_call_id"] == call["id"])?;
            let result: Value = serde_json::from_str(reply["content"].as_str()?).ok()?;
            let error = result["error"].as_str();
            let method = result["action_method"].as_str().filter(|m| {
                matches!(
                    *m,
                    "selection"
                        | "invoke"
                        | "toggle"
                        | "pointer"
                        | "native_button"
                        | "native_text"
                        | "uia_value"
                        | "unicode_ack"
                        | "unicode"
                )
            });
            Some(ActionDiagnostics {
                tool: tool.into(),
                action: action.into(),
                outcome: if result["permission_denied"] == true {
                    "denied"
                } else if error.is_some() {
                    "failed"
                } else if result["performed"] == true {
                    "dispatched"
                } else {
                    "observed"
                }
                .into(),
                error_category: error.map(|e| error_category(e).into()),
                method: method.map(str::to_owned),
                elements: result["elements"].as_array().map_or(0, Vec::len),
                inspection_limited: result["inspection_limited"] == true,
                scroll_moved: result["scroll_result"]["moved"].as_bool(),
                scroll_method: result["scroll_result"]["method"]
                    .as_str()
                    .filter(|v| matches!(*v, "uia" | "wheel" | "dom"))
                    .map(str::to_owned),
                scroll_at_boundary: result["scroll_result"]["at_boundary"].as_bool(),
                snapshot_bound: result["snapshot_bound"] == true,
                observation_offset: result["offset"].as_u64().unwrap_or(0).min(4095) as usize,
                decision_observation_offset: result["decision"]["observation_offset"]
                    .as_u64()
                    .unwrap_or(0)
                    .min(4095) as usize,
                decision_probability: probability(&result["decision"]["score"]["probability"]),
                decision_runner_up: probability(&result["decision"]["score"]["runner_up"]),
                decision_margin: result["decision"]["score"]["margin"]
                    .as_f64()
                    .filter(|v| v.is_finite() && (-1.0..=1.0).contains(v)),
                decision_threshold: probability(&result["decision"]["score"]["threshold"]),
                decision_candidates: result["decision"]["score"]["candidates"]
                    .as_u64()
                    .unwrap_or(0)
                    .min(24) as usize,
                decision_escalation_reason: decision_label(&result["decision"]["score"]["reason"]),
                decision_cached: result["decision"]["status"] == "cached_escalation"
                    || result["decision"]["fallback_reason"] == "cached_escalation",
                decision_mode: decision_label(&result["decision"]["mode"]),
                decision_status: decision_label(&result["decision"]["status"]),
                decision_route: decision_label(&result["decision"]["route"]),
                decision_fallback_reason: decision_label(&result["decision"]["fallback_reason"]),
                decision_vision: result["decision"]["vision"] == true,
                decision_backend: decision_label(&result["decision"]["backend"]),
                decision_runtime_loaded: result["decision"]["runtime_loaded"] == true,
                decision_vision_images: result["decision"]["vision_images"].as_u64().unwrap_or(0)
                    as usize,
                decision_inference_ms: result["decision"]["inference_ms"]
                    .as_f64()
                    .filter(|v| v.is_finite() && *v >= 0.0),
                decision_calls: result["decision"]["calls"].as_u64().unwrap_or(0) as usize,
                decision_executed: result["decision"]["executed"].as_u64().unwrap_or(0) as usize,
                decision_ms: result["decision"]["elapsed_ms"]
                    .as_f64()
                    .filter(|v| v.is_finite() && *v >= 0.0),
            })
        })
        .collect()
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct StepDiagnostics {
    pub step: usize,
    pub status: String,
    pub http_status: Option<u16>,
    pub input_tokens: Option<u64>,
    pub cached_tokens: Option<u64>,
    pub output_tokens: Option<u64>,
    pub prefill_tokens: Option<u64>,
    pub server_prefill_ms: Option<f64>,
    pub server_decode_ms: Option<f64>,
    pub first_response_byte_ms: Option<f64>,
    pub first_token_ms: Option<f64>,
    pub model_ms: Option<f64>,
    /// Includes permission waiting and native helper execution, separate from model latency.
    pub action_wait_ms: Option<f64>,
    pub text_bytes: usize,
    pub image_count: usize,
    pub image_encoded_bytes: usize,
    pub tool_schema_bytes: usize,
    pub messages: usize,
    /// Exact matching message prefix, NOT a measurement of the server's token cache.
    pub matching_prefix_messages: Option<usize>,
    pub previous_messages: Option<usize>,
    pub earlier_context_changed: bool,
    pub tool_config_changed: bool,
    pub screenshots_replaced: usize,
    pub messages_removed: usize,
    pub tool_calls: usize,
    pub usage_inconsistent: bool,
    #[serde(default)]
    pub actions: Vec<ActionDiagnostics>,
}
impl StepDiagnostics {
    pub fn cache_percent(&self) -> Option<f64> {
        let total = self.input_tokens?;
        let cached = self.cached_tokens?;
        (total > 0 && cached <= total).then_some(cached as f64 * 100.0 / total as f64)
    }
    fn observe(&mut self, value: &Value) {
        let usage = &value["usage"];
        let count = |key: &str| usage[key].as_u64();
        if let Some(n) = count("prompt_tokens").or_else(|| count("input_tokens")) {
            self.input_tokens = Some(n);
        }
        if let Some(n) = count("completion_tokens").or_else(|| count("output_tokens")) {
            self.output_tokens = Some(n);
        }
        if let Some(n) = usage["prompt_tokens_details"]["cached_tokens"]
            .as_u64()
            .or_else(|| usage["input_tokens_details"]["cached_tokens"].as_u64())
        {
            self.cached_tokens = Some(n);
        }
        // llama.cpp prompt_n is newly evaluated tokens, not the full prompt or a cache hit count.
        let timings = &value["timings"];
        if let Some(n) = timings["prompt_n"].as_u64() {
            self.prefill_tokens = Some(n);
        }
        fn duration(value: &Value) -> Option<f64> {
            value.as_f64().filter(|n| n.is_finite() && *n >= 0.0)
        }
        if let Some(n) = duration(&timings["prompt_ms"]) {
            self.server_prefill_ms = Some(n);
        }
        if let Some(n) = duration(&timings["predicted_ms"]) {
            self.server_decode_ms = Some(n);
        }
        self.usage_inconsistent = matches!((self.input_tokens,self.cached_tokens),(Some(total),Some(cached)) if cached>total);
    }
}

#[derive(Default)]
pub struct Prefix {
    messages: Vec<[u8; 32]>,
    tools: Option<[u8; 32]>,
    initialized: bool,
}
impl Prefix {
    fn digest(value: &Value) -> [u8; 32] {
        Sha256::digest(serde_json::to_vec(value).unwrap_or_default()).into()
    }
    pub fn observe(&mut self, body: &Value, step: usize) -> StepDiagnostics {
        let messages = body["messages"]
            .as_array()
            .map(Vec::as_slice)
            .unwrap_or_default();
        let hashes = messages.iter().map(Self::digest).collect::<Vec<_>>();
        let tools = Self::digest(&serde_json::json!([
            body["tools"],
            body["tool_choice"],
            body["parallel_tool_calls"]
        ]));
        let matching = self
            .messages
            .iter()
            .zip(&hashes)
            .take_while(|(a, b)| a == b)
            .count();
        let mut result = StepDiagnostics {
            step,
            messages: messages.len(),
            matching_prefix_messages: self.initialized.then_some(matching),
            previous_messages: self.initialized.then_some(self.messages.len()),
            earlier_context_changed: self.initialized && matching < self.messages.len(),
            tool_config_changed: self.initialized && self.tools != Some(tools),
            tool_schema_bytes: body.get("tools").map(|v| v.to_string().len()).unwrap_or(0),
            ..Default::default()
        };
        for message in messages {
            match &message["content"] {
                Value::String(text) => result.text_bytes += text.len(),
                Value::Array(parts) => {
                    for part in parts {
                        if part["type"] == "image_url" {
                            result.image_count += 1;
                            result.image_encoded_bytes +=
                                part["image_url"]["url"].as_str().map_or(0, str::len);
                        } else {
                            result.text_bytes += part["text"].as_str().map_or(0, str::len);
                        }
                    }
                }
                _ => {}
            }
            result.text_bytes += message
                .get("tool_calls")
                .map(|v| v.to_string().len())
                .unwrap_or(0);
        }
        self.messages = hashes;
        self.tools = Some(tools);
        self.initialized = true;
        result
    }
}

pub struct Trace<'a, F: FnMut(Metrics)> {
    pub data: StepDiagnostics,
    pub started: Instant,
    action_started: Option<Instant>,
    sink: &'a RefCell<F>,
    cancel: &'a Arc<AtomicBool>,
}
impl<'a, F: FnMut(Metrics)> Trace<'a, F> {
    pub fn new(data: StepDiagnostics, sink: &'a RefCell<F>, cancel: &'a Arc<AtomicBool>) -> Self {
        Self {
            data,
            started: Instant::now(),
            action_started: None,
            sink,
            cancel,
        }
    }
    pub fn observe(&mut self, value: &Value) {
        self.data.observe(value);
        let message = &value["choices"][0];
        let has_generated = |v: &Value| {
            v["content"].as_str().is_some_and(|s| !s.is_empty())
                || v["reasoning_content"]
                    .as_str()
                    .is_some_and(|s| !s.is_empty())
                || v["reasoning"].as_str().is_some_and(|s| !s.is_empty())
                || v["tool_calls"].as_array().is_some_and(|v| !v.is_empty())
        };
        if self.data.first_token_ms.is_none() && has_generated(&message["delta"]) {
            self.data.first_token_ms = Some(self.started.elapsed().as_secs_f64() * 1000.0);
        }
    }
    pub fn bytes_received(&mut self) {
        if self.data.first_response_byte_ms.is_none() {
            self.data.first_response_byte_ms = Some(self.started.elapsed().as_secs_f64() * 1000.0);
        }
    }
    pub fn start_actions(&mut self) {
        self.action_started = Some(Instant::now());
    }
    pub fn actions_done(&mut self) {
        self.data.action_wait_ms = self
            .action_started
            .map(|t| t.elapsed().as_secs_f64() * 1000.0);
    }
    pub fn response_done(&mut self, tools: usize) {
        self.data.model_ms = Some(self.started.elapsed().as_secs_f64() * 1000.0);
        self.data.tool_calls = tools;
        self.data.status = "response".into();
    }
}
impl<F: FnMut(Metrics)> Drop for Trace<'_, F> {
    fn drop(&mut self) {
        if self.data.action_wait_ms.is_none() {
            self.actions_done();
        }
        if self.data.model_ms.is_none() {
            self.data.model_ms = Some(self.started.elapsed().as_secs_f64() * 1000.0);
        }
        if self.cancel.load(Ordering::Acquire) {
            self.data.status = "cancelled".into();
        } else if self.data.status.is_empty() || self.data.status == "response" {
            self.data.status = "error".into();
        }
        (self.sink.borrow_mut())(Metrics {
            step: Some(self.data.clone()),
            ..Default::default()
        });
    }
}
#[cfg(test)]
mod tests {
    #[test]
    fn fresh_request_preconditions_are_not_provider_failures() {
        assert_eq!(
            super::error_category("Take a fresh observation in this request before acting"),
            "missing_observation"
        );
        assert_eq!(
            super::error_category("Stale snapshot; use a fresh observation"),
            "stale_observation"
        );
        assert_eq!(
            super::error_category("Ambiguous original target in refreshed observation"),
            "ambiguous_target"
        );
    }

    #[test]
    fn decision_diagnostics_allow_only_known_labels() {
        assert_eq!(
            super::decision_label(&serde_json::json!("remote")).as_deref(),
            Some("remote")
        );
        assert_eq!(
            super::decision_label(&serde_json::json!("local")).as_deref(),
            Some("local")
        );
        assert!(super::decision_label(&serde_json::json!("private user content")).is_none());
    }

    use super::*;
    use serde_json::json;
    #[test]
    fn action_diagnostics_do_not_retain_private_arguments_or_errors() {
        let calls = vec![
            json!({"id":"c","function":{"name":"computer_use","arguments":json!({"action":"type","text":"PRIVATE_SECRET","element_name":"PRIVATE_SECRET"}).to_string()}}),
        ];
        let replies = vec![
            json!({"tool_call_id":"c","content":json!({"error":"Text verification failed PRIVATE_SECRET"}).to_string()}),
        ];
        let rows = actions(&calls, &replies);
        assert_eq!(rows[0].error_category.as_deref(), Some("text_verification"));
        assert!(
            !serde_json::to_string(&rows)
                .unwrap()
                .contains("PRIVATE_SECRET")
        );
    }
    #[test]
    fn server_counts_preserve_zero_missing_and_invalid_values() {
        let mut row = StepDiagnostics::default();
        row.observe(&json!({"usage":{"prompt_tokens":30000,"completion_tokens":100,"prompt_tokens_details":{"cached_tokens":975}},"timings":{"prompt_n":29025,"prompt_ms":16000,"predicted_ms":1000}}));
        assert_eq!(row.input_tokens, Some(30000));
        assert_eq!(row.cached_tokens, Some(975));
        assert_eq!(row.prefill_tokens, Some(29025));
        assert_eq!(row.cache_percent(), Some(3.25));
        row.observe(&json!({"choices":[{"delta":{"content":"next"}}]}));
        assert_eq!(row.cached_tokens, Some(975));
        row.observe(&json!({"usage":{"prompt_tokens_details":{"cached_tokens":0}}}));
        assert_eq!(row.cache_percent(), Some(0.0));
        row.observe(
            &json!({"usage":{"prompt_tokens":1,"prompt_tokens_details":{"cached_tokens":2}}}),
        );
        assert!(row.usage_inconsistent);
        assert_eq!(row.cache_percent(), None);
        let mut missing = StepDiagnostics::default();
        missing.observe(&json!({"timings":{"prompt_n":100,"prompt_ms":50},"tokens_cached":999}));
        assert_eq!(missing.input_tokens, None);
        assert_eq!(missing.cached_tokens, None);
        assert_eq!(missing.prefill_tokens, Some(100));
        missing.observe(&json!({"usage":{"prompt_tokens":-2,"completion_tokens":"100","prompt_tokens_details":{"cached_tokens":null}},"timings":{"prompt_ms":-20}}));
        assert_eq!(missing.input_tokens, None);
        assert_eq!(missing.output_tokens, None);
        assert_eq!(missing.server_prefill_ms, Some(50.0));
    }
    #[test]
    fn prefix_reports_append_rewrite_removal_and_tool_changes_without_contents() {
        let mut prefix = Prefix::default();
        let mut body = json!({"messages":[{"role":"user","content":"PRIVATE_PROMPT"}],"tools":[{"private":"PRIVATE_SCHEMA"}],"tool_choice":"required"});
        let first = prefix.observe(&body, 1);
        assert_eq!(first.matching_prefix_messages, None);
        body["messages"]
            .as_array_mut()
            .unwrap()
            .push(json!({"role":"assistant","content":"private result"}));
        let append = prefix.observe(&body, 2);
        assert_eq!(append.matching_prefix_messages, Some(1));
        assert!(!append.earlier_context_changed);
        body["messages"][0]["content"] =
            json!([{"type":"image_url","image_url":{"url":"PRIVATE_IMAGE"}}]);
        body["tool_choice"] = json!("auto");
        let changed = prefix.observe(&body, 3);
        assert!(changed.earlier_context_changed);
        assert!(changed.tool_config_changed);
        assert_eq!(changed.image_count, 1);
        assert_eq!(changed.image_encoded_bytes, 13);
        body["messages"].as_array_mut().unwrap().pop();
        let removed = prefix.observe(&body, 4);
        assert!(removed.earlier_context_changed);
        let text = serde_json::to_string(&changed).unwrap();
        assert!(!text.contains("PRIVATE"));
        assert!(!text.contains("private result"));
    }
    #[test]
    fn stream_first_token_ignores_role_headers_usage_and_json_completion() {
        let rows = RefCell::new(Vec::new());
        let sink = RefCell::new(|m: Metrics| rows.borrow_mut().push(m.step.unwrap()));
        let cancel = Arc::new(AtomicBool::new(false));
        {
            let mut trace = Trace::new(StepDiagnostics::default(), &sink, &cancel);
            trace.observe(&json!({"choices":[{"delta":{"role":"assistant","content":""}}]}));
            assert_eq!(trace.data.first_token_ms, None);
            trace.observe(&json!({"choices":[{"message":{"content":"whole JSON response"}}]}));
            assert_eq!(trace.data.first_token_ms, None);
            trace.observe(&json!({"choices":[{"delta":{"tool_calls":[{"index":0,"function":{"name":"computer_use"}}]}}]}));
            assert!(trace.data.first_token_ms.is_some());
            trace.observe(&json!({"usage":{"input_tokens":50,"output_tokens":3,"input_tokens_details":{"cached_tokens":25}}}));
            trace.response_done(1);
            trace.start_actions();
            trace.actions_done();
            trace.data.status = "tools".into();
        }
        let rows = rows.borrow();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].cache_percent(), Some(50.0));
        assert!(rows[0].action_wait_ms.is_some());
        assert_eq!(rows[0].status, "tools");
    }
    #[test]
    fn failed_and_cancelled_requests_still_emit_one_safe_row() {
        let rows = RefCell::new(Vec::new());
        let sink = RefCell::new(|m: Metrics| rows.borrow_mut().push(m.step.unwrap()));
        let cancel = Arc::new(AtomicBool::new(false));
        {
            let mut trace = Trace::new(StepDiagnostics::default(), &sink, &cancel);
            trace.data.http_status = Some(503);
        }
        {
            let mut trace = Trace::new(StepDiagnostics::default(), &sink, &cancel);
            trace.response_done(1);
            trace.start_actions();
            cancel.store(true, Ordering::Release);
        }
        let rows = rows.borrow();
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0].status, "error");
        assert_eq!(rows[0].http_status, Some(503));
        assert_eq!(rows[1].status, "cancelled");
        assert!(rows[1].action_wait_ms.is_some());
    }
}

#[cfg(test)]
mod transport_tests {
    use super::*;
    use crate::{
        ai::{ControlTools, ask_with_control, prepare},
        config::Settings,
    };
    use serde_json::json;
    #[test]
    fn streamed_usage_only_trailers_are_one_diagnostic_per_model_round() {
        let server = tiny_http::Server::http("127.0.0.1:0").unwrap();
        let url = format!("http://{}/v1", server.server_addr());
        let worker = std::thread::spawn(move || {
            for round in 0..2 {
                let req = server
                    .recv_timeout(std::time::Duration::from_secs(8))
                    .unwrap()
                    .unwrap();
                let delta = if round == 0 {
                    json!({"tool_calls":[{"index":0,"id":"c1","type":"function","function":{"name":"computer_use","arguments":"{\"action\":\"windows\"}"}}]})
                } else {
                    json!({"content":"Verified response."})
                };
                let body = format!(
                    "data: {}\n\ndata: {}\n\ndata: {}\n\ndata: [DONE]\n\n",
                    json!({"choices":[{"delta":{"role":"assistant","content":""}}]}),
                    json!({"choices":[{"delta":delta}]}),
                    json!({"choices":[],"usage":{"prompt_tokens":1000+round*500,"completion_tokens":20,"prompt_tokens_details":{"cached_tokens":900+round*100}},"timings":{"prompt_n":100+round*400,"prompt_ms":50,"predicted_ms":10}})
                );
                req.respond(tiny_http::Response::from_string(body).with_header(
                    tiny_http::Header::from_bytes("Content-Type", "text/event-stream").unwrap(),
                ))
                .unwrap();
            }
        });
        let s = Settings {
            llm_enabled: true,
            llm_model: "fixture".into(),
            llm_url: url,
            llm_context: 16384,
            computer_use_enabled: true,
            control_max_steps: 3,
            ..Default::default()
        };
        let control = Arc::new(ControlTools {
            computer: true,
            browser: false,
            history: Default::default(),
            call: Arc::new(|_, _| Ok(json!({"windows":[]}))),
        });
        let mut rows = Vec::new();
        let mut text = String::new();
        ask_with_control(
            &s,
            &prepare(&s, &[], "PRIVATE_INPUT").unwrap(),
            &Arc::new(AtomicBool::new(false)),
            false,
            None,
            Some(control),
            |s| text.push_str(&s),
            |m| {
                if let Some(row) = m.step {
                    rows.push(row)
                }
            },
        )
        .unwrap();
        worker.join().unwrap();
        assert_eq!(text, "Verified response.");
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0].input_tokens, Some(1000));
        assert_eq!(rows[0].cache_percent(), Some(90.0));
        assert_eq!(rows[1].input_tokens, Some(1500));
        assert_eq!(rows[1].cached_tokens, Some(1000));
        assert_eq!(rows[0].status, "tools");
        assert_eq!(rows[1].status, "answer");
        assert!(
            rows.iter()
                .all(|v| v.first_token_ms.is_some() && v.model_ms.is_some())
        );
        assert!(
            !serde_json::to_string(&rows)
                .unwrap()
                .contains("PRIVATE_INPUT")
        );
    }
}
