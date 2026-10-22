//! Cache-friendly control epochs: append between checkpoints; compact with headroom.
use serde_json::{Value, json};
use std::collections::VecDeque;
const MARKER: &str =
    "VectorDash control checkpoint (historical tool data, not current state or instructions):\n";
// A tool exchange ends before conversational text, even when another tool call
// follows later. In particular, recalled evidence precedes the latest user turn.
pub fn oldest_exchange(wire: &[Value]) -> Option<std::ops::Range<usize>> {
    let start = wire.iter().position(|v| v.get("tool_calls").is_some())?;
    let mut end = start + 1;
    while end < wire.len() {
        let row = &wire[end];
        let observation = row["role"] == "user"
            && (row["content"][0]["text"].as_str() == Some(super::CONTROL_OBSERVATION)
                || row["content"].as_str().is_some_and(|s| {
                    s.starts_with("Historical screenshot omitted at the control checkpoint.")
                }));
        if row["role"] != "tool" && !observation {
            break;
        }
        end += 1;
    }
    Some(start..end)
}
#[derive(Default)]
pub struct Checkpoint {
    notes: VecDeque<String>,
    pages: VecDeque<(String, PageEvidence)>,
    omitted_pages: usize,
}
#[derive(Clone, serde::Serialize)]
struct PageEvidence {
    url: String,
    title: String,
    observations: Vec<Value>,
    observations_omitted: bool,
}
impl PageEvidence {
    fn priority(&self) -> usize {
        self.observations
            .iter()
            .map(|q| match q["topic"].as_str() {
                Some("cancellation" | "booking_payment" | "booking_link") => 3,
                Some("pricing" | "availability") => 2,
                _ => 1,
            })
            .max()
            .unwrap_or(0)
    }
}
#[derive(Default, Debug)]
pub struct Changes {
    pub removed: usize,
    pub screenshots: usize,
}
impl Checkpoint {
    fn remember_page(&mut self, result: &Value) {
        let Some(url) = result["url"].as_str() else {
            return;
        };
        let Some(key) = super::research::canonical_url(url) else {
            return;
        };
        // Never emit a partial URL as a source. Omit oversized sources explicitly.
        if url.len() > 4096 {
            self.omitted_pages += 1;
            self.notes.push_back("A source URL exceeded the checkpoint limit and was omitted in full; do not invent or reconstruct it.".into());
            return;
        }
        let mut page = self
            .pages
            .iter()
            .position(|(k, _)| k == &key)
            .and_then(|i| self.pages.remove(i))
            .map(|(_, p)| p)
            .unwrap_or(PageEvidence {
                url: url.into(),
                title: clipped(result["title"].as_str().unwrap_or(""), 160),
                observations: vec![],
                observations_omitted: false,
            });
        let text = result["text"].as_str().unwrap_or("");
        let mut quotes = Vec::new();
        for line in text.lines().filter(|line| !line.trim().is_empty()) {
            let lower = line.to_lowercase();
            let topic = if lower.contains("cancel") || lower.contains("no-show") {
                "cancellation"
            } else if lower.contains("payment options") || lower.contains("payments accepted") {
                "accepted_payment_methods"
            } else if lower.contains("credit")
                || lower.contains("deposit")
                || lower.contains("card required")
                || lower.contains("card hold")
            {
                "booking_payment"
            } else if lower.contains('$') || lower.contains("menu") {
                "pricing"
            } else if lower.contains("available")
                || lower.contains("timeslot")
                || lower.contains(" pm")
                || lower.contains(" am")
            {
                "availability"
            } else {
                continue;
            };
            let quote = json!({"topic":topic,"quotation":clipped(line.trim(),280),"text_offset":result["text_offset"].as_u64().unwrap_or(0),"historical":true,"verification":"not independently verified"});
            if !quotes.contains(&quote) {
                quotes.push(quote);
            }
        }
        if quotes.is_empty() && page.observations.is_empty() && !text.is_empty() {
            quotes.push(json!({"topic":"page_excerpt","quotation":clipped(text,280),"historical":true,"verification":"not independently verified"}));
        }
        // Booking terms may be carried by an observed widget/link payload, not body text.
        // Preserve the complete link without treating its destination as inspected.
        let mut links = Vec::new();
        if let Some(frames) = result["embedded_frames"].as_array() {
            for frame in frames {
                links.push((
                    frame["url"].as_str().unwrap_or(""),
                    frame["title"].as_str().unwrap_or(""),
                ));
            }
        }
        if let Some(elements) = result["elements"].as_array() {
            for element in elements {
                links.push((
                    element["href"].as_str().unwrap_or(""),
                    element["name"].as_str().unwrap_or(""),
                ));
            }
        }
        for (url, title) in links {
            if url.len() > 4096 || super::research::canonical_url(url).is_none() {
                continue;
            }
            let description = format!("{title} {url}").to_lowercase();
            if ["book", "reserv", "cancel", "deposit", "policy"]
                .iter()
                .any(|term| description.contains(term))
            {
                let quote = json!({"topic":"booking_link","observed_url":url,"title":clipped(title,120),"historical":true,"verification":"link observed only; target contents not inspected"});
                if !quotes.contains(&quote) {
                    quotes.push(quote);
                }
            }
        }
        for quote in quotes {
            if page.observations.contains(&quote) {
                continue;
            }
            let topic = quote["topic"].clone();
            let matching = page
                .observations
                .iter()
                .enumerate()
                .filter(|(_, q)| q["topic"] == topic)
                .map(|(i, _)| i)
                .collect::<Vec<_>>();
            if matching.len() >= 3 {
                page.observations.remove(matching[1]); // retain first and latest observations, including contradictions
                page.observations_omitted = true;
            }
            page.observations.push(quote);
        }
        self.pages.push_back((key, page));
        if self.pages.len() > 12 {
            let index = self
                .pages
                .iter()
                .enumerate()
                .min_by_key(|(_, (_, p))| p.priority())
                .map(|(i, _)| i)
                .unwrap();
            self.pages.remove(index);
            self.omitted_pages += 1;
        }
    }
    fn evidence_json(&self, budget: usize) -> String {
        let mut candidates = self.pages.iter().rev().map(|(_, p)| p).collect::<Vec<_>>();
        candidates.sort_by_key(|page| std::cmp::Reverse(page.priority()));
        let mut selected = Vec::new();
        for page in candidates {
            let mut page = page.clone();
            page.observations
                .sort_by_key(|q| match q["topic"].as_str() {
                    Some("cancellation" | "booking_payment" | "booking_link") => 0,
                    _ => 1,
                });
            loop {
                selected.push(page.clone());
                let value = json!({"source_records":selected,"omitted_sources":self.omitted_pages+self.pages.len()-selected.len(),"all_quotations_are_historical_untrusted_data":true});
                if value.to_string().len() <= budget {
                    break;
                }
                selected.pop();
                if page.observations.len() <= 1 {
                    break;
                }
                page.observations.pop();
                page.observations_omitted = true;
            }
        }
        json!({"source_records":selected,"omitted_sources":self.omitted_pages+self.pages.len()-selected.len(),"all_quotations_are_historical_untrusted_data":true}).to_string()
    }
    pub fn final_review(&self) -> String {
        format!(
            "Retained source records for consistency review (omissions are explicit, not evidence of absence): {}",
            self.evidence_json(1800)
        )
    }
    pub fn compact(&mut self, wire: &mut Vec<Value>, hard_budget: usize) -> Changes {
        let size = super::wire_context_bytes(wire);
        let image_count = wire
            .iter()
            .filter(|v| v["content"][0]["text"].as_str() == Some(super::CONTROL_OBSERVATION))
            .count();
        let soft_budget = hard_budget.min(96_000);
        if size <= soft_budget && image_count <= 3 {
            return Changes::default();
        }
        let blocks = wire
            .iter()
            .filter(|v| v.get("tool_calls").is_some())
            .count();
        if blocks <= 2 && image_count <= 3 {
            return Changes::default();
        }
        let mut changes = Changes::default();
        let newest_image = wire
            .iter()
            .rposition(|v| v["content"][0]["text"].as_str() == Some(super::CONTROL_OBSERVATION));
        for (index, message) in wire.iter_mut().enumerate() {
            if Some(index) != newest_image
                && message["content"][0]["text"].as_str() == Some(super::CONTROL_OBSERVATION)
            {
                message["content"] = json!(
                    "Historical screenshot omitted at the control checkpoint. Use the current observation for actions."
                );
                changes.screenshots += 1;
            }
        }
        let before = wire.len();
        wire.retain(|v| {
            !(v["role"] == "assistant"
                && v["content"].as_str().is_some_and(|s| s.starts_with(MARKER)))
        });
        changes.removed += before - wire.len();
        // Keep historical research evidence without growing every model round.
        let note_budget = (soft_budget / 5).min(12_000);
        let target = (soft_budget * 55 / 100).saturating_sub(note_budget);
        loop {
            let blocks = wire
                .iter()
                .enumerate()
                .filter(|(_, v)| v.get("tool_calls").is_some())
                .map(|(i, _)| i)
                .collect::<Vec<_>>();
            if blocks.len() <= 2 || super::wire_context_bytes(wire) <= target {
                break;
            }
            let removed = wire
                .drain(oldest_exchange(wire).expect("tool exchange exists"))
                .collect::<Vec<_>>();
            changes.removed += removed.len();
            if let Some(calls) = removed[0]["tool_calls"].as_array() {
                for call in calls {
                    let Some(reply) = removed
                        .iter()
                        .find(|v| v["role"] == "tool" && v["tool_call_id"] == call["id"])
                    else {
                        continue;
                    };
                    let Ok(result) =
                        serde_json::from_str::<Value>(reply["content"].as_str().unwrap_or(""))
                    else {
                        continue;
                    };
                    let Ok(args) = serde_json::from_str::<Value>(
                        call["function"]["arguments"].as_str().unwrap_or(""),
                    ) else {
                        continue;
                    };
                    let action = args["action"].as_str().unwrap_or("");
                    if call["function"]["name"] == "browser_use" {
                        self.remember_page(&result);
                        if result.get("error").is_some() || result.get("progress_warning").is_some()
                        {
                            let url = args["url"].as_str().filter(|url| url.len() <= 4096);
                            self.notes.push_back(json!({"historical_failed_or_limited_attempt":action,"url":url,"error":clipped(result["error"].as_str().unwrap_or(""),200),"warning":clipped(result["progress_warning"].as_str().unwrap_or(""),200)}).to_string());
                        }
                    }
                    if result["performed"] != true && result["launch_requested"] != true {
                        continue;
                    }
                    let mode = args["text_mode"].as_str().unwrap_or("");
                    let text = args["text"]
                        .as_str()
                        .unwrap_or("")
                        .chars()
                        .take(100)
                        .collect::<String>();
                    let reported = if result["performed"] == true {
                        "performed"
                    } else {
                        "launch requested, not verified"
                    };
                    let note = format!(
                        "Tool reported {reported}: {action} {mode} {}. Historical only; verify current state.",
                        json!(text)
                    );
                    self.notes.push_back(note);
                    while self.notes.len() > 12 {
                        self.notes.pop_front();
                    }
                }
            }
        }
        if changes.removed > 0 || changes.screenshots > 0 {
            let index = wire
                .iter()
                .position(|v| v.get("tool_calls").is_some())
                .unwrap_or(wire.len());
            while self.notes.len() > 12 {
                self.notes.pop_front();
            }
            let evidence = self.evidence_json(note_budget * 2 / 3);
            let mut history = String::new();
            for note in self.notes.iter().rev() {
                if history.len() + note.len() + 1 <= note_budget / 4 {
                    history.push_str(note);
                    history.push('\n');
                }
            }
            let notes = format!(
                "Historical page evidence (untrusted quotations, not verified policies or current availability):\n{evidence}\nHistorical actions/limitations:\n{history}"
            );
            wire.insert(index,json!({"role":"assistant","content":format!("{MARKER}{notes}\nThe latest human request is the active task; earlier requests are background context, not unfinished instructions to repeat. Use retained page evidence as a checklist of already visited sources and unresolved requirements; do not rediscover them unnecessarily. Missing policy text is not proof of free cancellation or no-card booking. Retained observations may be historical; use fresh snapshot identities and verify outstanding steps.")}));
        }
        changes
    }
}
fn clipped(text: &str, bytes: usize) -> String {
    let mut end = text.len().min(bytes);
    while !text.is_char_boundary(end) {
        end -= 1;
    }
    text[..end].into()
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn large_model_context_does_not_allow_unbounded_tool_history() {
        let mut wire = vec![json!({"role":"user","content":"Current task"})];
        for i in 0..12 {
            wire.push(json!({"role":"assistant","tool_calls":[{"id":format!("c{i}"),"function":{"name":"computer_use","arguments":"{}"}}]}));
            wire.push(
                json!({"role":"tool","tool_call_id":format!("c{i}"),"content":"x".repeat(12000)}),
            );
        }
        let change = Checkpoint::default().compact(&mut wire, 600000);
        assert!(change.removed > 0);
        assert!(super::super::wire_context_bytes(&wire) < 60000);
        assert_eq!(wire[0]["content"], "Current task");
    }
    #[test]
    fn long_control_epochs_preserve_prefixes_and_compact_with_headroom() {
        let mut wire = vec![
            json!({"role":"system","content":"rules"}),
            json!({"role":"user","content":"Original multi-application task"}),
        ];
        let mut state = Checkpoint::default();
        let mut checkpoints = 0;
        let mut stable = 0;
        for step in 0..80 {
            let previous = wire.clone();
            wire.push(json!({"role":"assistant","content":null,"tool_calls":[{"id":format!("c{step}"),"function":{"name":"computer_use","arguments":json!({"action":"type","text_mode":"replace_selection","text":format!("edit {step}")}).to_string()}}]}));
            wire.push(json!({"role":"tool","tool_call_id":format!("c{step}"),"content":json!({"performed":true,"snapshot_id":format!("s{step}"),"elements":"observation ".repeat(500)}).to_string()}));
            let change = state.compact(&mut wire, 90_000);
            if change.removed > 0 {
                checkpoints += 1;
                assert!(super::super::wire_context_bytes(&wire) < 52_000);
            } else {
                stable += 1;
                assert_eq!(&wire[..previous.len()], previous.as_slice());
            }
            assert_eq!(wire[1]["content"], "Original multi-application task");
            for (i, row) in wire.iter().enumerate() {
                if row["role"] == "tool" {
                    assert!(wire[..i].iter().any(|a| {
                        a["tool_calls"]
                            .as_array()
                            .is_some_and(|c| c.iter().any(|v| v["id"] == row["tool_call_id"]))
                    }));
                }
            }
        }
        assert!(checkpoints < 20, "too many rewrites: {checkpoints}");
        assert!(stable > 60);
        assert!(wire.iter().any(|v| {
            v["content"]
                .as_str()
                .is_some_and(|s| s.starts_with(MARKER) && s.contains("edit"))
        }));
    }
    #[test]
    fn compaction_preserves_followups_between_recalled_and_current_tools() {
        for question in [
            "Now append a punchline",
            "Save as cow.txt",
            "Save cow.txt to Desktop",
        ] {
            let mut wire = vec![json!({"role":"user","content":"Original completed task"})];
            for i in 0..4 {
                wire.push(json!({"role":"assistant","tool_calls":[{"id":format!("old{i}"),"function":{"arguments":"{}"}}]}));
                wire.push(json!({"role":"tool","tool_call_id":format!("old{i}"),"content":"x".repeat(14000)}));
            }
            let answer = json!({"role":"assistant","content":"Previous task completed"});
            let request = json!({"role":"user","content":question});
            wire.extend([answer.clone(), request.clone()]);
            for i in 0..3 {
                wire.push(json!({"role":"assistant","tool_calls":[{"id":format!("new{i}"),"function":{"arguments":"{}"}}]}));
                wire.push(
                    json!({"role":"tool","tool_call_id":format!("new{i}"),"content":"current".repeat(2000)}),
                );
            }
            let changes = Checkpoint::default().compact(&mut wire, 48000);
            assert!(changes.removed > 0);
            let a = wire.iter().position(|v| v == &answer).unwrap();
            assert_eq!(wire[a + 1], request);
            assert!(wire.iter().any(|v| v["tool_calls"][0]["id"] == "new2"));
            for row in wire.iter().filter(|v| v["role"] == "tool") {
                assert!(wire.iter().any(|v| {
                    v["tool_calls"]
                        .as_array()
                        .is_some_and(|calls| calls.iter().any(|c| c["id"] == row["tool_call_id"]))
                }));
            }
        }
    }
    #[test]
    fn hard_budget_exchange_boundary_keeps_latest_request_and_attachment() {
        let mut wire = vec![
            json!({"role":"assistant","tool_calls":[{"id":"a"}]}),
            json!({"role":"tool","tool_call_id":"a","content":"old result"}),
            json!({"role":"assistant","content":"Old answer"}),
            json!({"role":"user","content":[{"type":"text","text":"Work on this image instead"},{"type":"image_url","image_url":{"url":"user image"}}]}),
            json!({"role":"assistant","tool_calls":[{"id":"b"}]}),
            json!({"role":"tool","tool_call_id":"b","content":"new result"}),
        ];
        let expected = wire[2..].to_vec();
        let range = oldest_exchange(&wire).unwrap();
        wire.drain(range);
        assert_eq!(wire, expected);
    }
    #[test]
    fn screenshots_are_immutable_between_periodic_checkpoints() {
        let mut wire = vec![json!({"role":"user","content":"task"})];
        let mut state = Checkpoint::default();
        for index in 0..4 {
            let before = wire.clone();
            wire.push(json!({"role":"assistant","tool_calls":[{"id":format!("i{index}"),"function":{"arguments":"{}"}}]}));
            wire.push(json!({"role":"tool","tool_call_id":format!("i{index}"),"content":"{}"}));
            wire.push(json!({"role":"user","content":[{"type":"text","text":super::super::CONTROL_OBSERVATION},{"type":"image_url","image_url":{"url":format!("image{index}")}}]}));
            let changes = state.compact(&mut wire, 90_000);
            if index < 3 {
                assert_eq!(changes.screenshots, 0);
                assert_eq!(&wire[..before.len()], before.as_slice());
            } else {
                assert_eq!(changes.screenshots, 3);
                assert_eq!(
                    wire.last().unwrap()["content"][1]["image_url"]["url"],
                    "image3"
                );
            }
        }
    }
}

#[cfg(test)]
mod replay_fixture {
    use super::*;
    #[test]
    fn export_general_cache_policy_probe_when_requested() {
        let Ok(path) = std::env::var("VECTORDASH_CACHE_REPLAY") else {
            return;
        };
        let mut output = serde_json::Map::new();
        for policy in ["rolling", "checkpoint"] {
            let mut wire = vec![
                json!({"role":"system","content":"This is a synthetic prompt-cache test, not a task to execute. Return only OK."}),
                json!({"role":"user","content":format!("Synthetic cache policy {policy}. No actions are to be executed.")}),
            ];
            let mut checkpoint = Checkpoint::default();
            let mut bodies = Vec::new();
            for step in 0..16 {
                let data = json!({"snapshot_id":format!("snapshot_{step}"),"elements":(0..30).map(|index|json!({"id":index,"role":"button","name":format!("Example control {index}"),"value":"Synthetic interface observation. ".repeat(4)})).collect::<Vec<_>>()});
                wire.push(json!({"role":"assistant","content":null,"tool_calls":[{"id":format!("c{step}"),"type":"function","function":{"name":"computer_use","arguments":"{\"action\":\"inspect\"}"}}]}));
                wire.push(json!({"role":"tool","tool_call_id":format!("c{step}"),"content":data.to_string()}));
                if policy == "checkpoint" {
                    checkpoint.compact(&mut wire, 48_000);
                } else {
                    while super::super::wire_context_bytes(&wire) > 48_000 {
                        let blocks = wire
                            .iter()
                            .enumerate()
                            .filter(|(_, v)| v.get("tool_calls").is_some())
                            .map(|(i, _)| i)
                            .collect::<Vec<_>>();
                        if blocks.len() < 2 {
                            break;
                        }
                        wire.drain(blocks[0]..blocks[1]);
                    }
                }
                bodies.push(json!({"messages":wire,"temperature":0,"max_tokens":8,"stream":false,"tools":[{"type":"function","function":{"name":"computer_use","description":"Synthetic benchmark observations only; never execute actions.","parameters":{"type":"object","properties":{"action":{"type":"string"}},"required":["action"]}}}],"tool_choice":"none"}));
            }
            output.insert(policy.into(), json!(bodies));
        }
        std::fs::write(path, serde_json::to_vec(&output).unwrap()).unwrap();
    }
    #[test]
    fn research_evidence_and_failed_sources_survive_repeated_checkpoints() {
        let mut checkpoint = Checkpoint::default();
        let mut wire = vec![
            json!({"role":"user","content":"Research only; do not book. Need no-card booking and free cancellation."}),
        ];
        for i in 0..20 {
            let id = format!("read{i}");
            wire.push(json!({"role":"assistant","tool_calls":[{"id":id,"function":{"name":"browser_use","arguments":json!({"action":"snapshot"}).to_string()}}]}));
            let value = if i == 0 {
                json!({"error":"Page blocked; use another verified source"})
            } else {
                json!({"url":"https://example.com/restaurant","title":"Restaurant fixture","text":format!("Cancellation terms unverified. Credit card policy not stated.\n{}", "filler ".repeat(1000))})
            };
            wire.push(json!({"role":"tool","tool_call_id":id,"content":value.to_string()}));
            checkpoint.compact(&mut wire, 24000);
        }
        let text = wire
            .iter()
            .filter_map(|m| m["content"].as_str())
            .collect::<Vec<_>>()
            .join("\n");
        assert!(text.contains("Page blocked"));
        assert!(text.contains("Historical page evidence"));
        assert!(text.contains("Credit card policy not stated"));
        assert!(text.contains("do not book"));
        assert!(checkpoint.pages.len() <= 12);
        assert!(checkpoint.notes.len() <= 12);
        assert!(!text.contains("Cancellation terms verified"));
    }
    #[test]
    fn complete_source_urls_and_policy_quotes_are_never_cut_mid_record() {
        let mut state = Checkpoint::default();
        let url = format!(
            "https://example.com/booking?date=2026-10-30&token={}",
            "z".repeat(900)
        );
        state.remember_page(&json!({"url":url,"title":"Source","text":"Cancellation fee is $20 with six hours notice.\nPayment options: Visa.\n7:45 PM available"}));
        for _ in 0..10 {
            state.remember_page(
                &json!({"url":url,"title":"Source","text":"Menu and navigation only"}),
            );
        }
        let packed = state.evidence_json(4000);
        let decoded: Value = serde_json::from_str(&packed).unwrap();
        assert_eq!(decoded["source_records"][0]["url"], url);
        assert!(packed.contains("Cancellation fee is $20"));
        assert!(packed.contains("accepted_payment_methods"));
        let tiny = state.evidence_json(300);
        let decoded: Value = serde_json::from_str(&tiny).unwrap();
        assert_eq!(decoded["source_records"].as_array().unwrap().len(), 0);
        assert_eq!(decoded["omitted_sources"], 1);
        assert!(!tiny.contains("https://"));
    }
    #[test]
    fn conflicting_quotes_survive_and_policy_sources_take_priority() {
        let mut state = Checkpoint::default();
        state.remember_page(
            &json!({"url":"https://example.com/policy","text":"Free cancellation until noon."}),
        );
        state.remember_page(&json!({"url":"https://example.com/policy","text":"Cancellation fee is $20 after noon."}));
        for i in 0..20 {
            state.remember_page(&json!({"url":format!("https://example.com/search?term={i}"),"text":"Navigation only"}));
        }
        let packed = state.evidence_json(1800);
        assert!(packed.len() <= 1800);
        assert!(packed.contains("Free cancellation until noon"));
        assert!(packed.contains("Cancellation fee is $20"));
        assert!(
            serde_json::from_str::<Value>(&packed).unwrap()["omitted_sources"]
                .as_u64()
                .unwrap()
                > 0
        );
    }
    #[test]
    fn booking_widget_payload_links_remain_whole_and_unverified() {
        let mut state = Checkpoint::default();
        let link =
            "https://example.com/widget?reservation=single&payload=%7B%22cancel_fee%22%3A20%7D";
        state.remember_page(&json!({"url":"https://example.com/restaurant","text":"Restaurant menu", "embedded_frames":[{"title":"Book a table","url":link}]}));
        for _ in 0..4 {
            state.remember_page(
                &json!({"url":"https://example.com/restaurant","text":"Menu navigation only"}),
            );
        }
        let value: Value = serde_json::from_str(&state.evidence_json(1800)).unwrap();
        let observations = value["source_records"][0]["observations"]
            .as_array()
            .unwrap();
        let quote = observations
            .iter()
            .find(|q| q["topic"] == "booking_link")
            .unwrap();
        assert_eq!(quote["observed_url"], link);
        assert_eq!(
            quote["verification"],
            "link observed only; target contents not inspected"
        );
    }
}
