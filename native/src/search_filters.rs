use anyhow::{Result, bail};
#[derive(Default, Debug, Clone)]
pub struct Filters {
    pub text: String,
    pub pattern: Option<String>,
    pub min_size: i64,
    pub max_size: i64,
    pub after: i64,
    pub before: i64,
    pub extension: Option<String>,
}
impl Filters {
    pub fn active(&self) -> bool {
        self.pattern.is_some()
            || self.extension.is_some()
            || self.min_size > 0
            || self.max_size < i64::MAX
            || self.after > 0
            || self.before < i64::MAX
    }
    pub fn parse(query: &str) -> Result<Self> {
        let mut f = Self {
            max_size: i64::MAX,
            before: i64::MAX,
            ..Self::default()
        };
        let mut words = vec![];
        for word in query.split_whitespace() {
            let low = word.to_lowercase();
            if let Some(v) = low.strip_prefix("size:") {
                let (op, v) = comparison(v);
                let n = size(v)?;
                match op {
                    ">" => f.min_size = n.saturating_add(1),
                    ">=" => f.min_size = n,
                    "<" => f.max_size = n.saturating_sub(1),
                    "<=" => f.max_size = n,
                    "=" => {
                        f.min_size = n;
                        f.max_size = n;
                    }
                    _ => unreachable!(),
                }
            } else if let Some(v) = low
                .strip_prefix("modified:")
                .or_else(|| low.strip_prefix("date:"))
            {
                let (op, v) = comparison(v);
                let date = if v == "today" {
                    chrono::Local::now().date_naive()
                } else {
                    chrono::NaiveDate::parse_from_str(v, "%Y-%m-%d").map_err(|_| {
                        anyhow::anyhow!(
                            "Use modified:YYYY-MM-DD, modified:>=YYYY-MM-DD or modified:today"
                        )
                    })?
                };
                use chrono::TimeZone;
                let midnight = chrono::Local
                    .from_local_datetime(&date.and_hms_opt(0, 0, 0).unwrap())
                    .earliest()
                    .ok_or_else(|| anyhow::anyhow!("Invalid local date"))?
                    .timestamp_nanos_opt()
                    .unwrap_or(i64::MAX);
                let end = chrono::Local
                    .from_local_datetime(
                        &date
                            .succ_opt()
                            .unwrap_or(date)
                            .and_hms_opt(0, 0, 0)
                            .unwrap(),
                    )
                    .earliest()
                    .map(|t| t.timestamp_nanos_opt().unwrap_or(i64::MAX))
                    .unwrap_or(i64::MAX);
                match op {
                    ">" => f.after = end,
                    ">=" => f.after = midnight,
                    "<" => f.before = midnight.saturating_sub(1),
                    "<=" => f.before = end.saturating_sub(1),
                    _ => {
                        f.after = midnight;
                        f.before = end.saturating_sub(1);
                    }
                }
            } else if let Some(v) = low.strip_prefix("ext:") {
                if v.is_empty() {
                    bail!("Use ext:pdf or ext:jpg,png");
                }
                f.extension = Some(v.trim_start_matches('.').into());
            } else if let Some(v) = word.strip_prefix("name:") {
                f.pattern = Some(v.to_lowercase());
            } else if word.contains(['*', '?']) && !word.contains(['\\', '/']) {
                f.pattern = Some(low);
            } else {
                words.push(word);
            }
        }
        f.text = words.join(" ");
        Ok(f)
    }
    pub fn uses_embeddings(&self) -> bool {
        !self.text.is_empty() && crate::db::path_query(&self.text).is_none()
    }
    pub fn matches(&self, name: &str, size: i64, mtime: i64) -> bool {
        size >= self.min_size
            && size <= self.max_size
            && mtime >= self.after
            && mtime <= self.before
            && self
                .pattern
                .as_ref()
                .is_none_or(|p| crate::config::wildcard_match(&name.to_lowercase(), p))
            && self.extension.as_ref().is_none_or(|e| {
                e.split(',')
                    .any(|x| name.to_lowercase().ends_with(&format!(".{x}")))
            })
    }
}
fn comparison(v: &str) -> (&str, &str) {
    for op in [">=", "<=", ">", "<", "="] {
        if let Some(v) = v.strip_prefix(op) {
            return (op, v);
        }
    }
    ("=", v)
}
fn size(v: &str) -> Result<i64> {
    let split = v
        .find(|c: char| !c.is_ascii_digit() && c != '.')
        .unwrap_or(v.len());
    let number: f64 = v[..split]
        .parse()
        .map_err(|_| anyhow::anyhow!("Use size:>10MB, size:<1GB or size:1024"))?;
    let scale = match &v[split..] {
        "" | "b" => 1.,
        "kb" | "kib" => 1024.,
        "mb" | "mib" => 1024. * 1024.,
        "gb" | "gib" => 1024. * 1024. * 1024.,
        _ => bail!("Size units: B, KB, MB or GB"),
    };
    if !number.is_finite() || number < 0. {
        bail!("Invalid size")
    }
    Ok((number * scale).min(i64::MAX as f64) as i64)
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn filenames_sizes_and_dates() {
        let f = Filters::parse("dog*.jpg size:>1KB modified:>=2026-01-01").unwrap();
        assert!(f.matches("dogs.jpg", 2048, i64::MAX));
        assert!(!f.matches("cat.jpg", 2048, i64::MAX));
        assert!(!f.matches("dogs.jpg", 512, i64::MAX));
        assert!(Filters::parse("size:banana").is_err());
        assert!(Filters::parse("modified:no-date").is_err());
    }
}

#[cfg(test)]
mod semantic_intent_tests {
    use super::*;
    #[test]
    fn explicit_filename_syntax_never_uses_embeddings() {
        for query in [
            "*nr*",
            r"*\*nr*",
            r"\Local\RHI",
            r"\RHI\nvngx_dlssnr.dll",
            "name:*.dll",
            "ext:dll",
            "size:>1MB",
        ] {
            assert!(!Filters::parse(query).unwrap().uses_embeddings(), "{query}");
        }
        assert!(Filters::parse("photos of dogs").unwrap().uses_embeddings());
        assert!(Filters::parse("dogs ext:jpg").unwrap().uses_embeddings());
    }
}
