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
    browser_visits: HashMap<u64, u8>,
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
            args["y"],
            args["keys"],
            args["text"],
            args["text_mode"],
            args["find_text"]
        ])
        .to_string()
    }
    fn navigation(args: &Value) -> Vec<(u64, &'static str, u8)> {
        if !matches!(args["action"].as_str(), Some("open" | "navigate")) {
            return vec![];
        }
        super::research::navigation_keys(args["url"].as_str().unwrap_or(""))
    }
    fn browser_visit(&mut self, args: &Value) -> (u8, &'static str) {
        let mut reported = (0, "exact");
        let mut ratio = 0;
        for (key, scope, limit) in Self::navigation(args) {
            if self.browser_visits.len() >= 768 && !self.browser_visits.contains_key(&key) {
                continue;
            }
            let count = self.browser_visits.entry(key).or_default();
            *count = count.saturating_add(1);
            let score = *count as usize * 100 / limit as usize;
            if score > ratio {
                reported = (*count, scope);
                ratio = score;
            }
        }
        reported
    }
    pub fn before(&self, tool: &str, args: &Value) -> anyhow::Result<()> {
        if tool == "browser_use" {
            for (key, _, limit) in Self::navigation(args) {
                anyhow::ensure!(
                    self.browser_visits.get(&key).copied().unwrap_or(0) < limit,
                    "Repeated navigation for the same source/search reached the research retry limit. No navigation ran. Use retained evidence, inspect the existing page or booking flow, change provider, or report the unresolved requirement. Tracking parameters, reordered query fields and filter/time variants are not a new research result."
                );
            }
        }
        if tool == "computer_use"
            && matches!(
                args["action"].as_str(),
                Some("click" | "scroll" | "type" | "key")
            )
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
        if tool == "browser_use" {
            self.browser_visit(args);
        }
        if tool == "computer_use"
            && matches!(
                args["action"].as_str(),
                Some("click" | "scroll" | "type" | "key")
            )
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
        if tool == "browser_use" {
            let (visits, scope) = self.browser_visit(args);
            if visits > 0 {
                result["navigation_repeats"] = serde_json::json!(visits);
                result["navigation_scope"] = serde_json::json!(scope);
            }
            if visits
                >= match scope {
                    "search" => 6,
                    "source" => 4,
                    _ => 3,
                }
            {
                result["progress_warning"] = serde_json::json!(
                    "This source/search has already been visited repeatedly in this request. Preserve established facts and unresolved requirements; change source or strategy rather than restart the same research. Further identical navigation will be blocked."
                );
            }
            return;
        }
        if tool != "computer_use" {
            return;
        }
        if result.get("error").is_some() {
            self.failed(tool, args);
            return;
        }
        let Some(elements) = result["elements"].as_array() else {
            return;
        };
        let mut h = std::collections::hash_map::DefaultHasher::new();
        // Snapshot hashes include layout and playback clocks; neither proves task progress.
        static CLOCK: std::sync::LazyLock<regex::Regex> = std::sync::LazyLock::new(|| {
            regex::Regex::new(r"\b\d{1,2}:\d{2}(?::\d{2})?\b").unwrap()
        });
        for element in elements {
            for field in [
                "role",
                "name",
                "value",
                "text",
                "toggle_state",
                "scroll_percent",
                "selected",
            ] {
                if matches!(field, "name" | "text" | "value") {
                    CLOCK
                        .replace_all(element[field].as_str().unwrap_or(""), "<time>")
                        .hash(&mut h);
                } else {
                    element[field].to_string().hash(&mut h);
                }
            }
        }
        let window = Self::window(tool, args);
        let hash = h.finish();
        let observation = format!("{}:{}", window, result["offset"].as_u64().unwrap_or(0));
        let prior = self.observations.insert(observation, hash);
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
    fn json_worker_errors_and_failed_shortcuts_are_counted() {
        let mut p = Progress::default();
        let a = json!({"action":"key","window_id":"w","keys":"Ctrl+T"});
        for _ in 0..3 {
            p.observe("computer_use", &a, &mut json!({"error":"Unsupported key"}));
        }
        assert!(p.before("computer_use", &a).is_err());
    }
    #[test]
    fn playback_clock_and_view_hash_do_not_reset_click_failures() {
        let mut p = Progress::default();
        let a = json!({"action":"click","window_id":"w","element":1});
        let mut r = result("Playing 0:01");
        r["view_state"] = json!(1);
        r["elements"][0]["text"] = json!("Playback 0:01");
        p.observe(
            "computer_use",
            &json!({"action":"inspect","window_id":"w"}),
            &mut r,
        );
        for i in 2..5 {
            r["elements"][0]["name"] = json!(format!("Playing 0:0{i}"));
            r["view_state"] = json!(i);
            r["elements"][0]["text"] = json!(format!("Playback 0:0{i}"));
            p.observe("computer_use", &a, &mut r);
        }
        assert!(p.before("computer_use", &a).is_err());
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
    #[test]
    fn distinct_inputs_do_not_share_failed_action_budget() {
        let mut p = Progress::default();
        let a = json!({"action":"key","window_id":"w","keys":"ESC"});
        for _ in 0..3 {
            p.failed("computer_use", &a);
        }
        assert!(p.before("computer_use", &a).is_err());
        assert!(
            p.before(
                "computer_use",
                &json!({"action":"key","window_id":"w","keys":"ENTER"})
            )
            .is_ok()
        );
        assert_ne!(
            Progress::action(&json!({"action":"type","text":"first"})),
            Progress::action(&json!({"action":"type","text":"second"}))
        );
    }
    #[test]
    fn browser_navigation_loops_are_bounded_without_discarding_query_dates() {
        let mut p = Progress::default();
        for i in 0..5 {
            let a = serde_json::json!({"action":"navigate","url":format!("https://example.com/book?date=2026-10-30#{}",i)});
            assert!(p.before("browser_use", &a).is_ok());
            let mut result = serde_json::json!({"url":"https://example.com/book"});
            p.observe("browser_use", &a, &mut result);
            if i >= 2 {
                assert!(result["progress_warning"].is_string());
            }
        }
        assert!(p.before("browser_use", &serde_json::json!({"action":"open","url":"https://example.com/book?date=2026-10-30"})).is_err());
        assert!(p.before("browser_use", &serde_json::json!({"action":"navigate","url":"https://example.com/book?date=2026-10-31"})).is_ok());
    }
    #[test]
    fn source_variants_share_a_limit_but_other_dates_and_venues_do_not() {
        let mut p = Progress::default();
        for i in 0..8 {
            let a = serde_json::json!({"action":"navigate","url":format!("https://example.com/book?dateTime=2026-10-30T{}:00&covers=2&id=venue1&filter={i}",17+i)});
            assert!(p.before("browser_use", &a).is_ok());
            p.observe("browser_use", &a, &mut serde_json::json!({}));
        }
        assert!(p.before("browser_use",&serde_json::json!({"action":"open","url":"https://example.com/book?id=venue1&date=2026-10-30&covers=2&filter=new"})).is_err());
        for url in [
            "https://example.com/book?id=venue2&date=2026-10-30&covers=2",
            "https://example.com/book?id=venue1&date=2026-10-31&covers=2",
            "https://example.com/book?id=venue1&date=2026-10-30&covers=3",
        ] {
            assert!(
                p.before(
                    "browser_use",
                    &serde_json::json!({"action":"navigate","url":url})
                )
                .is_ok()
            );
        }
    }
    #[test]
    fn search_query_variants_cannot_repeat_unbounded_discovery() {
        let mut p = Progress::default();
        for i in 0..12 {
            let a = serde_json::json!({"action":"navigate","url":format!("https://example.com/search?term=Italian{i}&date=2026-10-30")});
            assert!(p.before("browser_use", &a).is_ok());
            p.observe("browser_use", &a, &mut serde_json::json!({}));
        }
        assert!(p.before("browser_use",&serde_json::json!({"action":"open","url":"https://example.com/search?term=another&date=2026-10-30"})).is_err());
        assert!(Progress::default().before("browser_use",&serde_json::json!({"action":"open","url":"https://example.com/search?term=another&date=2026-10-30"})).is_ok());
    }
}
