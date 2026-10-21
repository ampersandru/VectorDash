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
}
#[derive(Default, Debug)]
pub struct Changes {
    pub removed: usize,
    pub screenshots: usize,
}
impl Checkpoint {
    pub fn compact(&mut self, wire: &mut Vec<Value>, hard_budget: usize) -> Changes {
        let size = super::wire_context_bytes(wire);
        let image_count = wire
            .iter()
            .filter(|v| v["content"][0]["text"].as_str() == Some(super::CONTROL_OBSERVATION))
            .count();
        let soft_budget = hard_budget;
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
        // Drop complete tool exchanges, not one message at a time. Reserve 3KB for notes.
        let target = (soft_budget * 55 / 100).saturating_sub(3000);
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
            let notes = self.notes.iter().cloned().collect::<Vec<_>>().join("\n");
            wire.insert(index,json!({"role":"assistant","content":format!("{MARKER}{notes}\nThe latest human request is the active task; earlier requests are background context, not unfinished instructions to repeat. Retained observations may be historical; use fresh snapshot identities and verify outstanding steps.")}));
        }
        changes
    }
}
#[cfg(test)]
mod tests {
    use super::*;
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
}
