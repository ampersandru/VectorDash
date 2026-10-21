//! Request-local binding for omitted observation IDs. Explicit IDs are never corrected.
use anyhow::{Context, Result, ensure};
use serde_json::{Value, json};
use std::collections::HashMap;
#[derive(Default)]
pub struct Observations {
    latest: HashMap<String, (String, String)>,
}
impl Observations {
    pub fn bind(&mut self, tool: &str, args: &mut Value) -> Result<bool> {
        if !matches!(
            args["action"].as_str(),
            Some("click" | "type" | "key" | "scroll")
        ) {
            return Ok(false);
        }
        let (window,id)=self.latest.get(tool).context("Take a fresh observation in this request before acting; previous-request snapshot IDs cannot be reused")?;
        ensure!(
            tool != "computer_use"
                || (!window.is_empty() && args["window_id"].as_str() == Some(window.as_str())),
            "Take a fresh observation of this exact window before acting"
        );
        let bound = args.get("snapshot_id").is_none();
        if bound {
            args["snapshot_id"] = json!(id);
        } else {
            ensure!(
                args["snapshot_id"].as_str() == Some(id.as_str()),
                "Stale desktop/browser snapshot; use the latest observation"
            );
        }
        self.latest.remove(tool);
        Ok(bound)
    }
    pub fn record(&mut self, tool: &str, args: &Value, result: &Value) {
        if result.get("error").is_some() {
            self.latest.remove(tool);
            return;
        }
        if let Some(id) = result["snapshot_id"].as_str() {
            let window = result["window_id"]
                .as_str()
                .or_else(|| args["window_id"].as_str())
                .unwrap_or("");
            self.latest.insert(tool.into(), (window.into(), id.into()));
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    fn read(o: &mut Observations, w: &str, id: &str) {
        o.record(
            "computer_use",
            &json!({"action":"inspect","window_id":w}),
            &json!({"snapshot_id":id,"window_id":w}),
        );
    }
    #[test]
    fn missing_id_uses_only_latest_unused_same_window_observation() {
        let mut o = Observations::default();
        read(&mut o, "w", "s1");
        read(&mut o, "w", "s2");
        let mut a = json!({"action":"type","window_id":"w","element":1});
        assert!(o.bind("computer_use", &mut a).unwrap());
        assert_eq!(a["snapshot_id"], "s2");
        assert!(o.bind("computer_use", &mut a).is_err());
    }
    #[test]
    fn wrong_window_explicit_stale_and_cross_request_ids_are_rejected() {
        let mut o = Observations::default();
        read(&mut o, "w", "fresh");
        for a in [
            json!({"action":"click","window_id":"other"}),
            json!({"action":"click","window_id":"w","snapshot_id":"old"}),
            json!({"action":"click","window_id":"w","snapshot_id":null}),
        ] {
            assert!(o.bind("computer_use", &mut a.clone()).is_err());
        }
        assert!(
            Observations::default()
                .bind(
                    "computer_use",
                    &mut json!({"action":"click","window_id":"w","snapshot_id":"fresh"})
                )
                .is_err()
        );
    }
    #[test]
    fn failed_read_invalidates_implicit_binding() {
        let mut o = Observations::default();
        read(&mut o, "w", "s");
        o.record(
            "computer_use",
            &json!({}),
            &json!({"error":"provider unavailable"}),
        );
        assert!(
            o.bind("computer_use", &mut json!({"action":"key","window_id":"w"}))
                .is_err()
        );
    }
}
