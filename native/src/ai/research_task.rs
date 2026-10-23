//! Request-local candidate workflow. Model labels/outcomes are claims, not verified facts.
use serde_json::{Value, json};
use std::collections::{BTreeMap, HashMap};
const STAGES: [&str; 5] = [
    "details",
    "pricing",
    "availability",
    "card_policy",
    "cancellation",
];
#[derive(Default)]
struct Stage {
    attempts: usize,
    status: String,
    note: String,
    closed: bool,
}
#[derive(Default)]
struct Candidate {
    name: String,
    stages: BTreeMap<String, Stage>,
    sources: Vec<String>,
    excluded: bool,
}
#[derive(Default)]
pub struct Task {
    pub enabled: bool,
    candidates: BTreeMap<String, Candidate>,
    active: Option<(String, String)>,
    aliases: HashMap<String, String>,
    discovery: usize,
    limit: usize,
}
fn name_key(name: &str) -> String {
    name.chars()
        .filter(|c| c.is_alphanumeric())
        .flat_map(char::to_lowercase)
        .collect()
}
fn short(value: &str, max: usize) -> String {
    value.chars().take(max).collect()
}
fn listing(url: &str) -> bool {
    let Ok(url) = reqwest::Url::parse(url) else {
        return true;
    };
    let p = url.path().to_ascii_lowercase();
    p == "/s" || p.contains("/search") || p.contains("/neighborhood") || p.contains("/cuisine")
}
fn aliases(url: &str) -> Vec<String> {
    let Some(key) = super::research::canonical_url(url) else {
        return vec![];
    };
    let mut out = vec![key.clone()];
    if let Ok(u) = reqwest::Url::parse(&key) {
        for (k, v) in u.query_pairs() {
            if matches!(
                k.to_ascii_lowercase().as_str(),
                "rid" | "restref" | "restaurantid" | "restaurant_id" | "venue_id"
            ) {
                out.push(format!("{}:venue:{}", u.host_str().unwrap_or(""), v));
            }
        }
    }
    out
}
impl Task {
    pub fn new(enabled: bool) -> Self {
        Self {
            enabled,
            limit: 3,
            ..Default::default()
        }
    }
    pub fn before(&mut self, args: &Value, meta: Option<&Value>) -> anyhow::Result<()> {
        use anyhow::ensure;
        if let Some(meta) = meta {
            ensure!(
                meta.is_object() && meta.to_string().len() <= 2048,
                "Invalid research metadata"
            );
            let name = meta["candidate"].as_str().unwrap_or("").trim();
            let stage = meta["stage"].as_str().unwrap_or("");
            ensure!(
                !name.is_empty() && name.len() <= 120 && STAGES.contains(&stage),
                "Research metadata needs a candidate name and stage: details, pricing, availability, card_policy or cancellation"
            );
            if let Some(limit) = meta["candidate_limit"].as_u64() {
                ensure!(
                    (1..=12).contains(&limit),
                    "Research candidate_limit must be 1-12"
                );
                if self.candidates.is_empty() {
                    self.limit = limit as usize;
                }
            }
            let key = name_key(name);
            ensure!(
                !key.is_empty(),
                "Research candidate needs a meaningful name"
            );
            ensure!(
                self.candidates.contains_key(&key)
                    || (self.candidates.values().filter(|c| !c.excluded).count() < self.limit
                        && self.candidates.len() < 12),
                "Research candidate list is full. Finish the missing checks for existing candidates instead of restarting discovery. Mark unresolved stages unknown or blocked with their observed source; do not invent evidence"
            );
            let status = meta["outcome"].as_str().unwrap_or("checking");
            ensure!(
                matches!(
                    status,
                    "checking" | "observed" | "unknown" | "blocked" | "excluded"
                ),
                "Invalid research outcome"
            );
            self.enabled = true;
            let candidate = self
                .candidates
                .entry(key.clone())
                .or_insert_with(|| Candidate {
                    name: name.into(),
                    ..Default::default()
                });
            let state = candidate.stages.entry(stage.into()).or_default();
            let closing = matches!(status, "unknown" | "blocked" | "excluded")
                && !state.closed
                && args["action"] == "snapshot";
            ensure!(
                state.attempts < 5 || closing,
                "Research stage limit reached. No action ran. Close this criterion once with outcome unknown/blocked on a snapshot, then choose a different missing criterion/candidate"
            );
            if closing {
                state.closed = true;
            }
            state.status = status.into();
            state.note = short(meta["note"].as_str().unwrap_or(""), 200);
            candidate.excluded = status == "excluded" || candidate.excluded;
            self.active = Some((key, stage.into()));
        }
        if !self.enabled {
            return Ok(());
        }
        if let Some((key, stage)) = &self.active {
            let state = &self.candidates[key].stages[stage];
            let closing = meta.is_some()
                && matches!(state.status.as_str(), "unknown" | "blocked" | "excluded")
                && args["action"] == "snapshot";
            ensure!(
                state.attempts < 5 || closing,
                "Research stage limit reached: five attempts for this candidate/criterion. No action ran. Attach research metadata to a snapshot to mark this stage unknown or blocked, then check a different missing criterion/candidate or summarize. Do not restart discovery"
            );
        } else {
            ensure!(
                self.discovery < 10
                    || !matches!(args["action"].as_str(), Some("open" | "navigate")),
                "Research discovery limit reached. No navigation ran. Select up to the requested number of already observed candidates using research metadata, then inspect each candidate's missing criteria rather than run more broad searches"
            );
        }
        Ok(())
    }
    pub fn observe(&mut self, args: &Value, result: &mut Value, annotated: bool) {
        if !self.enabled {
            return;
        }
        let mut bound = None;
        if let Some(url) = result["url"].as_str().filter(|u| u.len() <= 4096) {
            if !listing(url) {
                bound = aliases(url)
                    .iter()
                    .find_map(|k| self.aliases.get(k).cloned());
                if bound.is_none() {
                    if let Some((key, _)) = &self.active {
                        let title = name_key(result["title"].as_str().unwrap_or(""));
                        if !title.is_empty() && title.contains(key) {
                            bound = Some(key.clone());
                        }
                    }
                }
                if let Some(key) = &bound {
                    let candidate = self.candidates.get_mut(key).unwrap();
                    if !candidate.sources.iter().any(|s| s == url) {
                        candidate.sources.push(url.into());
                        if candidate.sources.len() > 4 {
                            candidate.sources.remove(1);
                        }
                    }
                    let mut observed = vec![url.to_owned()];
                    for group in ["elements", "embedded_frames"] {
                        if let Some(rows) = result[group].as_array() {
                            for row in rows {
                                let link = row["href"]
                                    .as_str()
                                    .or_else(|| row["url"].as_str())
                                    .unwrap_or("");
                                let label = row["name"]
                                    .as_str()
                                    .or_else(|| row["title"].as_str())
                                    .unwrap_or("")
                                    .to_lowercase();
                                if link.len() <= 4096
                                    && (link.contains("/booking/")
                                        || link.contains("/restref/")
                                        || link.contains("widgets.")
                                        || label.contains("reserve")
                                        || label.contains("book a table"))
                                {
                                    observed.push(link.into());
                                }
                            }
                        }
                    }
                    let identities = observed
                        .iter()
                        .flat_map(|source| aliases(source).into_iter().skip(1))
                        .collect::<std::collections::HashSet<_>>();
                    if identities.len() > 1 {
                        result["research_identity_conflict"] = json!(true);
                        observed.truncate(1); // related booking widgets must not be attributed to this venue
                    }
                    for source in observed {
                        for alias in aliases(&source) {
                            if self.aliases.len() < 256 {
                                // Conflicting identity is never silently rebound.
                                if let Some(prior) = self.aliases.get(&alias) {
                                    if prior != key {
                                        result["research_identity_conflict"] = json!(true);
                                        continue;
                                    }
                                }
                                self.aliases.insert(alias, key.clone());
                            }
                        }
                    }
                    result["research_binding"] = json!({"candidate":candidate.name,"identity":"observed URL/link association; model label, not independently verified"});
                }
            }
        }
        if let Some(key) = &bound {
            if let Some((prior, stage)) = self.active.clone() {
                if &prior != key {
                    self.candidates
                        .get_mut(key)
                        .unwrap()
                        .stages
                        .entry(stage.clone())
                        .or_insert_with(|| Stage {
                            status: "checking".into(),
                            ..Default::default()
                        });
                    self.active = Some((key.clone(), stage));
                }
            }
        }
        if let Some((key, stage)) = &self.active {
            let state = self
                .candidates
                .get_mut(key)
                .unwrap()
                .stages
                .get_mut(stage)
                .unwrap();
            state.attempts += 1;
            result["research_stage"] = json!(stage);
            result["research_stage_attempts"] = json!(state.attempts);
            if annotated || state.attempts >= 3 {
                result["research_next"] = json!(
                    "Continue the active candidate's missing checks. Observed/model-reported outcomes are not independently verified. After five attempts, mark this criterion unknown/blocked rather than restart searches. Finish or explicitly mark all stages before the final comparison."
                );
            }
        } else if matches!(args["action"].as_str(), Some("open" | "navigate")) {
            self.discovery += 1;
        }
        result["research_candidate_count"] = json!(self.candidates.len());
        result["research_bound"] = json!(bound.is_some());
    }
    pub fn summary(&self, budget: usize) -> Value {
        if !self.enabled {
            return Value::Null;
        }
        let mut rows=self.candidates.values().map(|c|json!({"candidate":c.name,"model_reported_exclusion":c.excluded,"sources":c.sources,"criteria":STAGES.iter().map(|stage|{
            let s=c.stages.get(*stage);((*stage).to_owned(),json!({"status":s.map(|s|s.status.as_str()).unwrap_or("not_checked"),"attempts":s.map(|s|s.attempts).unwrap_or(0),"model_note":s.map(|s|s.note.as_str()).unwrap_or("")}))
        }).collect::<BTreeMap<_,_>>()})).collect::<Vec<_>>();
        // Retain every candidate name and status; drop whole optional fields, never URL fragments.
        let mut value = json!({"candidate_records":rows,"all_statuses_are_model_reports_not_verification":true});
        if value.to_string().len() > budget {
            for row in &mut rows {
                row["sources"] = json!([]);
                row["source_details_omitted"] = json!(true);
                for stage in STAGES {
                    row["criteria"][stage]
                        .as_object_mut()
                        .unwrap()
                        .remove("model_note");
                }
            }
            value = json!({"candidate_records":rows,"all_statuses_are_model_reports_not_verification":true});
        }
        if value.to_string().len() > budget {
            for row in &mut rows {
                for stage in STAGES {
                    row["criteria"][stage] = row["criteria"][stage]["status"].clone();
                }
            }
            value = json!({"candidate_records":rows,"all_statuses_are_model_reports_not_verification":true});
        }
        if value.to_string().len() > budget {
            value = json!({"candidate_names":self.candidates.values().map(|c|c.name.as_str()).collect::<Vec<_>>(),"criterion_details_omitted":true,"all_statuses_are_model_reports_not_verification":true});
        }
        value
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn read() -> Value {
        json!({"action":"snapshot"})
    }
    fn meta(name: &str, stage: &str) -> Value {
        json!({"candidate":name,"stage":stage})
    }
    #[test]
    fn discovery_and_each_criterion_are_bounded_with_one_unknown_closure() {
        let mut t = Task::new(true);
        let nav = json!({"action":"navigate","url":"https://example.com/search"});
        for _ in 0..10 {
            t.before(&nav, None).unwrap();
            t.observe(&nav, &mut json!({}), false);
        }
        assert!(t.before(&nav, None).is_err());
        let a = meta("Manzoni", "availability");
        for _ in 0..5 {
            t.before(&read(), Some(&a)).unwrap();
            t.observe(&read(), &mut json!({}), true);
        }
        assert!(t.before(&read(), Some(&a)).is_err());
        let closing = json!({"candidate":"Manzoni","stage":"availability","outcome":"unknown","note":"No matching slot was verified"});
        t.before(&read(), Some(&closing)).unwrap();
        t.observe(&read(), &mut json!({}), true);
        assert!(t.before(&read(), Some(&closing)).is_err());
        assert!(t.before(&read(), Some(&meta("Manzoni", "pricing"))).is_ok());
    }
    #[test]
    fn observed_booking_id_keeps_anonymous_flow_attached_to_candidate() {
        let mut t = Task::new(true);
        t.before(&read(), Some(&meta("Manzoni", "details")))
            .unwrap();
        let mut profile = json!({"url":"https://example.com/manzoni","title":"Manzoni Restaurant","elements":[{"name":"Reserve","href":"https://example.com/booking/details?rid=20233&correlationId=first"}]});
        t.observe(&read(), &mut profile, true);
        assert_eq!(profile["research_binding"]["candidate"], "Manzoni");
        let mut flow = json!({"url":"https://example.com/booking/availability?rid=20233&correlationId=second","title":"Reservation details"});
        t.before(&read(), None).unwrap();
        t.observe(&read(), &mut flow, false);
        assert_eq!(flow["research_binding"]["candidate"], "Manzoni");
        let mut unrelated = json!({"url":"https://example.com/booking/details?rid=999","title":"Reservation details"});
        t.observe(&read(), &mut unrelated, true);
        assert!(unrelated.get("research_binding").is_none());
    }
    #[test]
    fn excluded_candidates_are_retained_while_a_replacement_can_be_selected() {
        let mut t = Task::new(true);
        let a = json!({"candidate":"Aperto","stage":"details","candidate_limit":1,"outcome":"excluded","note":"Observed permanently closed banner"});
        t.before(&read(), Some(&a)).unwrap();
        t.observe(&read(), &mut json!({}), true);
        t.before(&read(), Some(&meta("Manzoni", "details")))
            .unwrap();
        assert!(
            t.before(&read(), Some(&meta("Another candidate", "details")))
                .is_err()
        );
        let state = t.summary(2200).to_string();
        assert!(state.contains("Aperto") && state.contains("Manzoni"));
        assert!(state.contains("model_reported_exclusion"));
    }
    #[test]
    fn generic_listings_do_not_prove_candidate_identity_and_summaries_keep_names() {
        let mut t = Task::new(true);
        t.before(&read(), Some(&meta("Manzoni", "details")))
            .unwrap();
        let mut listing = json!({"url":"https://example.com/search?term=Manzoni","title":"Manzoni and other restaurants"});
        t.observe(&read(), &mut listing, true);
        assert!(!listing["research_bound"].as_bool().unwrap());
        for name in ["La Ciccia", "Delfina"] {
            t.before(&read(), Some(&meta(name, "details"))).unwrap();
        }
        let summary = t.summary(500).to_string();
        for name in ["Manzoni", "La Ciccia", "Delfina"] {
            assert!(summary.contains(name));
        }
        assert!(summary.len() <= 500);
    }
    #[test]
    fn mixed_booking_widgets_do_not_assign_other_venues_to_the_candidate() {
        let mut t = Task::new(true);
        t.before(&read(), Some(&meta("Manzoni", "details")))
            .unwrap();
        let mut profile = json!({"url":"https://example.com/manzoni","title":"Manzoni Restaurant","elements":[{"name":"Reserve","href":"https://example.com/booking?rid=1"},{"name":"Reserve nearby venue","href":"https://example.com/booking?rid=2"}]});
        t.observe(&read(), &mut profile, true);
        assert_eq!(profile["research_identity_conflict"], true);
        let mut flow =
            json!({"url":"https://example.com/booking?rid=2","title":"Reservation details"});
        t.observe(&read(), &mut flow, false);
        assert!(flow.get("research_binding").is_none());
    }
}
