//! Thin client for the skills.sh API.
//!
//! Compiled only when the `skills-sh` cargo feature is enabled. The whole module is gated
//! via `#[cfg(feature = "skills-sh")]` at the `mod skills_sh;` declaration in main.rs, so
//! no per-item attributes are needed here.
//!
//! Endpoints we use:
//! - `GET /api/v1/skills/search?q=<query>&limit=<n>` — fuzzy/semantic search.
//!
//! Auth: skills.sh requires an API key on all endpoints today (its public site claims an
//! unauth tier, but the actual endpoints return 401). We read it from
//! `ZSKILLS_SKILLS_SH_API_KEY` and surface a clear error when missing rather than letting
//! federated search silently fail.

use anyhow::{Context, Result};
use serde::Deserialize;

const API_KEY_ENV: &str = "ZSKILLS_SKILLS_SH_API_KEY";

/// `skills.sh` apex serves a 307 redirect to `www.skills.sh`; we hit the canonical host
/// directly to avoid the redirect hop. Redirect-following is also enabled so future DNS
/// changes don't break us.
const BASE_URL: &str = "https://www.skills.sh";
const USER_AGENT: &str = concat!("zskills/", env!("CARGO_PKG_VERSION"));

#[derive(Debug, Deserialize, Clone)]
#[allow(dead_code)]
pub struct SearchHit {
    pub id: String,
    pub slug: String,
    pub name: String,
    pub source: String,
    #[serde(default)]
    pub installs: u64,
    #[serde(rename = "sourceType", default)]
    pub source_type: Option<String>,
    #[serde(rename = "installUrl", default)]
    pub install_url: Option<String>,
}

#[derive(Debug, Deserialize)]
struct SearchResponse {
    #[serde(default)]
    data: Vec<SearchHit>,
}

/// Returns true if the user has configured a skills.sh API key.
pub fn has_api_key() -> bool {
    std::env::var(API_KEY_ENV)
        .map(|v| !v.trim().is_empty())
        .unwrap_or(false)
}

/// Search skills.sh for skills matching `query`. `limit` is clamped to 1..=200 by the API.
pub fn search(query: &str, limit: u32) -> Result<Vec<SearchHit>> {
    let Some(api_key) = std::env::var(API_KEY_ENV)
        .ok()
        .filter(|v| !v.trim().is_empty())
    else {
        anyhow::bail!(
            "skills.sh requires an API key. Set {} (get one from https://www.skills.sh/account).",
            API_KEY_ENV
        );
    };
    let client = reqwest::blocking::Client::builder()
        .user_agent(USER_AGENT)
        .redirect(reqwest::redirect::Policy::limited(5))
        .build()
        .context("building reqwest client")?;
    let url = format!("{}/api/v1/skills/search", BASE_URL);
    let resp = client
        .get(&url)
        .bearer_auth(&api_key)
        .query(&[("q", query), ("limit", &limit.to_string())])
        .send()
        .with_context(|| format!("calling skills.sh search ({})", url))?;
    let status = resp.status();
    if status == reqwest::StatusCode::UNAUTHORIZED || status == reqwest::StatusCode::FORBIDDEN {
        anyhow::bail!(
            "skills.sh rejected the API key in {} (HTTP {}). Check the value is current.",
            API_KEY_ENV,
            status.as_u16()
        );
    }
    let resp = resp
        .error_for_status()
        .context("skills.sh search returned an error status")?;
    let parsed: SearchResponse = resp
        .json()
        .context("decoding skills.sh search response as JSON")?;
    Ok(parsed.data)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_search_response_shape() {
        let raw = r#"{
            "data": [{
                "id": "vercel-labs/agent-skills/next-js-development",
                "slug": "next-js-development",
                "name": "Next.js Development",
                "source": "vercel-labs/agent-skills",
                "installs": 24531,
                "sourceType": "github",
                "installUrl": "https://github.com/vercel-labs/agent-skills"
            }],
            "query": "next",
            "searchType": "semantic",
            "count": 1,
            "durationMs": 42
        }"#;
        let parsed: SearchResponse = serde_json::from_str(raw).unwrap();
        assert_eq!(parsed.data.len(), 1);
        assert_eq!(parsed.data[0].slug, "next-js-development");
        assert_eq!(parsed.data[0].source, "vercel-labs/agent-skills");
        assert_eq!(parsed.data[0].installs, 24531);
    }

    #[test]
    fn tolerates_missing_optional_fields() {
        let raw = r#"{"data":[{"id":"x/y/z","slug":"z","name":"Z","source":"x/y"}]}"#;
        let parsed: SearchResponse = serde_json::from_str(raw).unwrap();
        assert_eq!(parsed.data[0].installs, 0);
        assert!(parsed.data[0].source_type.is_none());
        assert!(parsed.data[0].install_url.is_none());
    }
}
