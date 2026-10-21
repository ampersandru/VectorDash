//! Request-local no-progress guard. No observation content is logged or retained.
use serde_json::Value;
use std::{
    collections::{HashMap, HashSet},
    hash::{Hash, Hasher},
};
#[derive(Default)]
pub struct Progress {
    observations: HashMap<String, u64>,
    attempts: HashMap<(String, String), u8>,
    measured: HashSet<(String, String)>,
    stalled: HashMap<String, u8>,
}
impl Progress {
    fn window(tool: &str, args: &Value) -> String {
        format!("{tool}:{}", args["window_id"].as_str().unwrap_or(""))
    }
    fn action(args: &Value) -> String {
        serde_json::json!([
            args["action"],
            args["element"],
            args["automation_id"],
            args["element_name"],
            args["delta"],
            args["x"],
            args["y"]
        ])
        .to_string()
    }
    pub fn before(&self, tool: &str, args: &Value) -> anyhow::Result<()> {
        if tool == "computer_use"
            && matches!(args["action"].as_str(), Some("click" | "scroll" | "type"))
        {
            anyhow::ensure!(
                self.stalled
                    .get(&Self::window(tool, args))
                    .copied()
                    .unwrap_or(0)
                    < 6,
                "Six input attempts failed or made no observable progress in this window. Stop repeating the operation and report the limitation"
            );
            anyhow::ensure!(
                self.attempts
                    .get(&(Self::window(tool, args), Self::action(args)))
                    .copied()
                    .unwrap_or(0)
                    < 3,
                "Repeated action failed or produced no observable progress three times. No further action ran. Choose a different pane/control or a different strategy; for scrolling, try focused PageDown/PageUp. Do not repeat the same action."
            );
        }
        Ok(())
    }
    pub fn failed(&mut self, tool: &str, args: &Value) {
        if tool == "computer_use"
            && matches!(args["action"].as_str(), Some("click" | "scroll" | "type"))
        {
            let count = self
                .attempts
                .entry((Self::window(tool, args), Self::action(args)))
                .or_default();
            *count = count.saturating_add(1);
            let stalled = self.stalled.entry(Self::window(tool, args)).or_default();
            *stalled = stalled.saturating_add(1);
        }
    }
    pub fn observe(&mut self, tool: &str, args: &Value, result: &mut Value) {
        if tool != "computer_use" {
            return;
        }
        let Some(elements) = result["elements"].as_array() else {
            return;
        };
        let mut h = std::collections::hash_map::DefaultHasher::new();
        result["view_state"].to_string().hash(&mut h);
        for element in elements {
            for field in [
                "role",
                "name",
                "value",
                "text",
                "toggle_state",
                "scroll_percent",
            ] {
                element[field].to_string().hash(&mut h);
            }
        }
        let window = Self::window(tool, args);
        let hash = h.finish();
        let prior = self.observations.insert(window.clone(), hash);
        let movement = result["scroll_result"]["moved"].as_bool();
        let key = (window.clone(), Self::action(args));
        if movement == Some(false) {
            self.measured.insert(key.clone());
        }
        if movement == Some(true)
            || (prior.is_some_and(|old| old != hash)
                && matches!(
                    args["action"].as_str(),
                    Some("click" | "key" | "type" | "focus")
                ))
        {
            self.stalled.remove(&window);
            self.attempts.retain(|(w, _), _| w != &window);
            self.measured.retain(|(w, _)| w != &window);
        } else if movement != Some(false) && prior.is_some_and(|old| old != hash) {
            // Playback clocks and animated values can change during an inspect.
            // They do not undo a directly measured failed scroll.
            self.attempts
                .retain(|key, _| key.0 != window || self.measured.contains(key));
        }
        if matches!(args["action"].as_str(), Some("click" | "scroll"))
            && result["performed"] == true
            && movement != Some(true)
            && (movement == Some(false) || prior == Some(hash))
        {
            let stalled = self.stalled.entry(window.clone()).or_default();
            *stalled = stalled.saturating_add(1);
            let count = self
                .attempts
                .entry((window, Self::action(args)))
                .or_default();
            *count = count.saturating_add(1);
            if *count >= 2 {
                result["progress_warning"] = serde_json::json!(
                    "This action has repeated without an observable UI change. Verify the current state and change strategy; another identical attempt may be blocked. A sent action is not proof of success."
                );
            }
        }
        // Bounded even for a model that invents many window IDs/targets.
        if self.attempts.len() > 256 {
            self.attempts.clear();
            self.measured.clear();
        }
        if self.observations.len() > 128 {
            self.observations.clear();
            self.stalled.clear();
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    fn result(value: &str) -> Value {
        json!({"performed":true,"offset":0,"snapshot_id":"changing","elements":[{"id":99,"name":value,"role":"pane"}]})
    }
    #[test]
    fn varying_coordinates_do_not_evade_window_failure_limit() {
        let mut p = Progress::default();
        for x in 0..6 {
            p.failed(
                "computer_use",
                &json!({"action":"click","window_id":"w","x":x}),
            );
        }
        assert!(
            p.before(
                "computer_use",
                &json!({"action":"click","window_id":"w","x":99})
            )
            .is_err()
        );
    }
    #[test]
    fn repeated_dispatch_errors_are_bounded() {
        let mut p = Progress::default();
        let action = json!({"action":"click", "window_id":"w", "element":5});
        for _ in 0..3 {
            assert!(p.before("computer_use", &action).is_ok());
            p.failed("computer_use", &action);
        }
        assert!(p.before("computer_use", &action).is_err());
    }
    #[test]
    fn later_pages_participate_in_progress_tracking() {
        let mut p = Progress::default();
        let action = json!({"action":"click", "window_id":"w", "element":263});
        let mut page = result("same");
        page["offset"] = json!(240);
        p.observe(
            "computer_use",
            &json!({"action":"inspect","window_id":"w"}),
            &mut page,
        );
        for _ in 0..3 {
            p.observe("computer_use", &action, &mut page);
        }
        assert!(p.before("computer_use", &action).is_err());
    }
    #[test]
    fn unchanged_reads_do_not_reset_repeated_scroll_guard_but_real_changes_do() {
        let mut p = Progress::default();
        let read = json!({"action":"inspect","window_id":"w"});
        let scroll = json!({"action":"scroll","window_id":"w","delta":120});
        p.observe("computer_use", &read, &mut result("same"));
        for _ in 0..3 {
            assert!(p.before("computer_use", &scroll).is_ok());
            p.observe("computer_use", &scroll, &mut result("same"));
            p.observe("computer_use", &read, &mut result("same"));
        }
        assert!(p.before("computer_use", &scroll).is_err());
        assert!(
            p.before(
                "computer_use",
                &json!({"action":"scroll","window_id":"w","delta":-120})
            )
            .is_ok()
        );
        p.observe("computer_use", &read, &mut result("new content"));
        assert!(p.before("computer_use", &scroll).is_ok());
    }
    #[test]
    fn measured_scroll_movement_is_progress_even_with_an_unchanged_tree() {
        let mut p = Progress::default();
        let a = json!({"action":"scroll","window_id":"w","delta":120});
        p.observe(
            "computer_use",
            &json!({"window_id":"w"}),
            &mut result("same"),
        );
        for _ in 0..8 {
            let mut r = result("same");
            r["scroll_result"] = json!({"moved":true});
            p.observe("computer_use", &a, &mut r);
            assert!(p.before("computer_use", &a).is_ok());
        }
    }
    #[test]
    fn unrelated_clock_changes_do_not_reset_measured_failed_scrolls() {
        let mut p = Progress::default();
        let a = json!({"action":"scroll","window_id":"w","delta":120});
        for tick in 0..3 {
            let mut r = result(&format!("Clock {tick}"));
            r["scroll_result"] = json!({"moved":false});
            p.observe("computer_use", &a, &mut r);
            p.observe(
                "computer_use",
                &json!({"action":"inspect","window_id":"w"}),
                &mut result(&format!("Clock {}", tick + 1)),
            );
        }
        assert!(p.before("computer_use", &a).is_err());
        p.observe(
            "computer_use",
            &json!({"action":"key","window_id":"w"}),
            &mut result("Page down changed content"),
        );
        assert!(p.before("computer_use", &a).is_ok());
    }
    #[test]
    fn snapshot_ids_and_element_numbers_do_not_fake_progress() {
        let mut p = Progress::default();
        let a = json!({"action":"click","window_id":"w","element":4});
        p.observe(
            "computer_use",
            &json!({"window_id":"w"}),
            &mut result("same"),
        );
        for i in 0..3 {
            let mut r = result("same");
            r["snapshot_id"] = json!(i);
            r["elements"][0]["id"] = json!(i);
            p.observe("computer_use", &a, &mut r);
        }
        assert!(p.before("computer_use", &a).is_err());
        assert!(
            p.before(
                "computer_use",
                &json!({"action":"click","window_id":"other","element":4})
            )
            .is_ok()
        );
    }
}
