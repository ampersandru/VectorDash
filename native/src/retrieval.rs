//! Bounded evidence selection. Similarity is candidate evidence, never a factual confidence.
use crate::db::Hit;
use std::collections::HashMap;
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Channel {
    Any,
    Visual,
    Audio,
}
impl Channel {
    pub fn allows(self, kind: &str) -> bool {
        match self {
            Self::Any => true,
            Self::Visual => kind == "video",
            Self::Audio => kind == "video_audio",
        }
    }
}
#[derive(Debug)]
pub struct Plan {
    pub parts: Vec<(String, Channel)>,
    pub ordered: bool,
}
/// Only explicit video conjunction/order syntax triggers additional embeddings (at most three).
pub fn plan(query: &str, category: &str) -> Option<Plan> {
    let q = query.to_lowercase();
    if category != "Videos" && !["video", "画面", "视频"].iter().any(|w| q.contains(w)) {
        return None;
    }
    for marker in [
        ", while ",
        " while ",
        "，声音",
        "，英文音轨",
        "，音轨",
        "，声音却",
        "，声音在",
    ] {
        if let Some((a, b)) = q.split_once(marker) {
            if a.chars().count() >= 4 && b.chars().count() >= 4 {
                return Some(Plan {
                    parts: vec![(a.into(), Channel::Visual), (b.into(), Channel::Audio)],
                    ordered: false,
                });
            }
        }
    }
    let normalized = q
        .replace(" followed by ", " then ")
        .replace(", then ", " then ")
        .replace("紧接着切换为", " then ")
        .replace("紧接着", " then ")
        .replace("最后切到", " then ")
        .replace("然后", " then ");
    let normalized = if normalized.contains("依次") {
        normalized
            .replace('、', " then ")
            .replace("，最后", " then ")
    } else {
        normalized
    };
    let parts: Vec<_> = normalized
        .split(" then ")
        .map(str::trim)
        .filter(|p| !p.is_empty())
        .collect();
    if !(2..=3).contains(&parts.len()) || parts.iter().any(|p| p.chars().count() < 4) {
        return None;
    }
    Some(Plan {
        parts: parts
            .into_iter()
            .map(|p| (p.into(), Channel::Visual))
            .collect(),
        ordered: true,
    })
}
pub fn evidence_reason(kind: &str, media: bool, exact: bool) -> &'static str {
    match (kind, media, exact) {
        ("video", false, true) => "Matches text visible in this video frame",
        ("video", false, false) => "Matches OCR text from this video frame",
        ("video", true, _) => "Matches this video frame visually",
        ("video_audio", _, _) => "Matches audio near this timestamp",
        (_, _, true) => "Matches exact words and meaning",
        ("image", true, _) => "Matches the picture",
        (_, true, _) => "Matches this media moment",
        _ => "Matches the meaning of this passage",
    }
}
pub fn best_per_file(mut hits: Vec<Hit>) -> Vec<Hit> {
    hits.sort_by(|a, b| {
        b.score
            .total_cmp(&a.score)
            .then_with(|| a.seconds.total_cmp(&b.seconds))
            .then_with(|| a.kind.cmp(&b.kind))
    });
    let mut seen = std::collections::HashSet::new();
    hits.retain(|h| seen.insert(h.path.clone()));
    hits
}
/// Up to four moments per clause/file and at most 4^3 combinations per candidate.
/// Ordered queries require distinct increasing timestamps; AV clauses must be within 30s.
pub fn combine(base: Vec<Hit>, parts: Vec<Vec<Hit>>, ordered: bool) -> Vec<Hit> {
    let mut by_part: Vec<HashMap<String, Vec<Hit>>> = vec![];
    for mut hits in parts {
        hits.sort_by(|a, b| b.score.total_cmp(&a.score));
        let mut map: HashMap<String, Vec<Hit>> = HashMap::new();
        for h in hits {
            let list = map.entry(h.path.clone()).or_default();
            if list.len() < 4
                && h.seconds >= 0.0
                && h.score >= 0.35
                && list.first().is_none_or(|best| h.score >= best.score - 0.12)
            {
                list.push(h);
            }
        }
        by_part.push(map);
    }
    let mut output = HashMap::new();
    for mut h in best_per_file(base) {
        h.score -= 0.18;
        h.reason = "Partial evidence; all requested conditions are not established".into();
        output.insert(h.path.clone(), h);
    }
    if !(2..=3).contains(&by_part.len()) {
        return output.into_values().collect();
    }
    for (path, first) in &by_part[0] {
        if by_part.iter().any(|p| !p.contains_key(path)) {
            continue;
        }
        let mut best: Option<(f32, Vec<&Hit>)> = None;
        for a in first {
            for b in &by_part[1][path] {
                let third: Vec<Option<&Hit>> = if by_part.len() == 3 {
                    by_part[2][path].iter().map(Some).collect()
                } else {
                    vec![None]
                };
                for c in third {
                    let mut selected = vec![a, b];
                    if let Some(c) = c {
                        selected.push(c);
                    }
                    let valid = if ordered {
                        selected
                            .windows(2)
                            .all(|p| p[1].seconds > p[0].seconds + 0.1)
                    } else {
                        let lo = selected
                            .iter()
                            .map(|h| h.seconds)
                            .fold(f32::INFINITY, f32::min);
                        let hi = selected.iter().map(|h| h.seconds).fold(0.0, f32::max);
                        hi - lo <= 30.0
                    };
                    if !valid {
                        continue;
                    }
                    let weakest = selected
                        .iter()
                        .map(|h| h.score)
                        .fold(f32::INFINITY, f32::min);
                    let mean =
                        selected.iter().map(|h| h.score).sum::<f32>() / selected.len() as f32;
                    let score = 0.7 * weakest + 0.3 * mean;
                    if best.as_ref().is_none_or(|(s, _)| score > *s) {
                        best = Some((score, selected));
                    }
                }
            }
        }
        if let Some((score, selected)) = best {
            let mut h = selected[0].clone();
            h.score = score;
            h.evidence = selected
                .iter()
                .flat_map(|h| h.evidence.iter().cloned())
                .take(3)
                .collect();
            h.reason = if ordered {
                "Candidate visual evidence in the requested time order"
            } else {
                "Candidate visual and audio evidence within 30 seconds"
            }
            .into();
            h.text = selected
                .iter()
                .enumerate()
                .map(|(i, h)| {
                    format!(
                        "Evidence {} at {:.1}s — {}{}",
                        i + 1,
                        h.seconds,
                        h.reason,
                        if h.text.is_empty() {
                            String::new()
                        } else {
                            format!(": {}", h.text.chars().take(1000).collect::<String>())
                        }
                    )
                })
                .collect::<Vec<_>>()
                .join("\n");
            output.insert(path.clone(), h);
        }
    }
    output.into_values().collect()
}
#[cfg(test)]
mod tests {
    use super::*;
    fn hit(path: &str, t: f32, score: f32, kind: &str) -> Hit {
        Hit {
            evidence: Vec::new(),
            path: path.into(),
            kind: kind.into(),
            text: String::new(),
            score,
            reason: "candidate".into(),
            seconds: t,
            page: 0,
            line: 0,
        }
    }
    #[test]
    fn ordered_rerank_rejects_reverse_and_same_frame_controls() {
        let parts = vec![
            vec![
                hit("right", 0., 0.8, "video"),
                hit("reverse", 10., 0.9, "video"),
                hit("same", 0., 0.9, "video"),
            ],
            vec![
                hit("right", 10., 0.8, "video"),
                hit("reverse", 0., 0.9, "video"),
                hit("same", 0., 0.9, "video"),
            ],
        ];
        let out = combine(vec![], parts, true);
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].path, "right");
        assert_eq!(out[0].seconds, 0.);
        assert!(out[0].text.contains("10.0s"));
    }
    #[test]
    fn cross_modal_requires_both_and_proximity_and_weakest_link() {
        let out = combine(
            vec![],
            vec![
                vec![
                    hit("both", 5., 0.8, "video"),
                    hit("remote", 0., 0.9, "video"),
                    hit("visual_only", 0., 0.99, "video"),
                ],
                vec![
                    hit("both", 10., 0.7, "video_audio"),
                    hit("remote", 90., 0.9, "video_audio"),
                ],
            ],
            false,
        );
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].path, "both");
        assert!(out[0].score < 0.75);
        assert!(!Channel::Visual.allows("video_audio"));
        assert!(Channel::Visual.allows("video"));
    }
    #[test]
    fn query_plans_are_explicit_bounded_and_multilingual() {
        assert!(plan("dogs and cats", "Images").is_none());
        assert!(
            plan("video forest then a golden dog then ocean", "Videos")
                .unwrap()
                .ordered
        );
        assert_eq!(
            plan("画面有金色狗，声音在讲合唱排练", "Videos")
                .unwrap()
                .parts[1]
                .1,
            Channel::Audio
        );
        assert!(
            plan(
                "find video forest then dog then ocean then mountains",
                "Videos"
            )
            .is_none()
        );
        assert!(plan("find notes then edit them", "Documents").is_none());
    }
    #[test]
    fn evidence_labels_never_call_audio_a_visual_identifier() {
        assert!(evidence_reason("video_audio", true, false).contains("audio"));
        assert!(evidence_reason("video", false, true).contains("visible"));
    }
}
