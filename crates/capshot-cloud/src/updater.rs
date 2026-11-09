//! Update check against GitHub releases (winshot's updater).

use crate::{http_client, http_err, CloudError, Result};
use serde::Deserialize;

/// `owner/repo` to check, set at build time with `CAPSHOT_UPDATE_REPO`.
/// Without it the update check is off.
pub const REPO: Option<&str> = option_env!("CAPSHOT_UPDATE_REPO");
pub const CURRENT_VERSION: &str = env!("CARGO_PKG_VERSION");

#[derive(Debug, Clone, PartialEq)]
pub struct Release {
    pub version: semver::Version,
    pub tag: String,
    pub url: String,
    pub notes: String,
}

#[derive(Deserialize)]
struct GhRelease {
    tag_name: String,
    html_url: String,
    #[serde(default)]
    body: Option<String>,
    #[serde(default)]
    prerelease: bool,
    #[serde(default)]
    draft: bool,
}

fn parse_version(tag: &str) -> Option<semver::Version> {
    semver::Version::parse(tag.trim_start_matches(['v', 'V'])).ok()
}

/// Latest stable release of `repo` (GitHub's `/releases/latest` skips
/// pre-releases and drafts; they are rejected here too).
pub fn latest(repo: &str, api_base: &str) -> Result<Release> {
    let r = http_client()?
        .get(format!("{api_base}/repos/{repo}/releases/latest"))
        .header("Accept", "application/vnd.github+json")
        .timeout(std::time::Duration::from_secs(10))
        .send()
        .map_err(http_err)?;
    let status = r.status().as_u16();
    let body = r.text().unwrap_or_default();
    if !(200..300).contains(&status) {
        return Err(CloudError::Status { service: "GitHub", status, body });
    }
    let gh: GhRelease = serde_json::from_str(&body).map_err(|e| CloudError::Other(e.to_string()))?;
    if gh.prerelease || gh.draft {
        return Err(CloudError::Other("latest release is a pre-release".into()));
    }
    let version =
        parse_version(&gh.tag_name).ok_or_else(|| CloudError::Other(format!("unrecognised tag {}", gh.tag_name)))?;
    Ok(Release { version, tag: gh.tag_name, url: gh.html_url, notes: gh.body.unwrap_or_default() })
}

/// Whether `release` should be offered: newer than `current` and not the
/// version the user chose to skip.
pub fn should_offer(current: &str, release: &Release, skipped: &str) -> bool {
    let Some(cur) = parse_version(current) else { return false };
    if parse_version(skipped).is_some_and(|s| s == release.version) {
        return false;
    }
    release.version > cur
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mock;

    fn rel(v: &str) -> Release {
        Release { version: parse_version(v).unwrap(), tag: format!("v{v}"), url: String::new(), notes: String::new() }
    }

    #[test]
    fn offers_only_newer_unskipped_versions() {
        assert!(should_offer("1.5.0", &rel("1.6.0"), ""));
        assert!(!should_offer("1.6.0", &rel("1.6.0"), ""));
        assert!(!should_offer("1.7.0", &rel("1.6.0"), ""));
        assert!(!should_offer("1.5.0", &rel("1.6.0"), "v1.6.0"));
        assert!(should_offer("1.5.0", &rel("1.6.1"), "1.6.0"));
    }

    #[test]
    fn fetches_latest_release() {
        let m = mock::serve(vec![(
            200,
            r#"{"tag_name":"v2.1.0","html_url":"https://x/r","body":"Notes","prerelease":false}"#,
        )]);
        let r = latest("me/capshot", &m.url).unwrap();
        assert_eq!(r.version, semver::Version::new(2, 1, 0));
        assert_eq!(r.notes, "Notes");
        assert_eq!(m.requests.lock().unwrap()[0].path, "/repos/me/capshot/releases/latest");
    }

    #[test]
    fn rejects_prereleases() {
        let m = mock::serve(vec![(200, r#"{"tag_name":"v3.0.0-beta.1","html_url":"x","prerelease":true}"#)]);
        assert!(latest("me/capshot", &m.url).is_err());
    }
}
