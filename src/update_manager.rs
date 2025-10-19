use anyhow::Result;
use serde_json::Value;
use tracing::{debug, info};
use std::path::PathBuf;
use std::fs;
use std::env;
use std::time::{SystemTime, Duration};
use directories::ProjectDirs;

/// Small struct describing an available update.
pub struct UpdateAvailable {
    pub version: String,
    pub body: Option<String>,
    pub url: Option<String>,
}

/// Update severity level
pub enum UpdateLevel {
    Major,
    Minor,
    Patch,
    None,
    Unknown,
}

pub struct UpdateManager {
    owner: String,
    repo: String,
    bin_name: String,
    current_version: String,
}

impl UpdateManager {
    pub fn new(owner: &str, repo: &str, bin_name: &str, current_version: &str) -> Self {
        Self {
            owner: owner.to_string(),
            repo: repo.to_string(),
            bin_name: bin_name.to_string(),
            current_version: current_version.to_string(),
        }
    }

    /// Return path to cache file for release JSON, e.g. $XDG_CACHE_HOME/wake/release_check.json
    fn cache_file_path(&self) -> Option<PathBuf> {
        if let Some(proj) = ProjectDirs::from("com", "samba-rgb", "wake") {
            let cache_dir = proj.cache_dir();
            let _ = fs::create_dir_all(cache_dir);
            return Some(cache_dir.join("release_latest.json"));
        }
        // fallback to temp dir
        let mut p = env::temp_dir();
        p.push(format!("wake_release_{}_{}.json", self.owner, self.repo));
        Some(p)
    }

    /// Whether cache file is fresh (within `max_age` duration)
    fn is_cache_fresh(&self, path: &PathBuf, max_age: Duration) -> bool {
        if let Ok(meta) = fs::metadata(path) {
            if let Ok(mtime) = meta.modified() {
                if let Ok(elapsed) = SystemTime::now().duration_since(mtime) {
                    return elapsed <= max_age;
                }
            }
        }
        false
    }

    /// Check for an update by querying the GitHub Releases API asynchronously.
    /// Returns Ok(Some(UpdateAvailable)) if a newer release tag is different from current_version.
    /// Returns Ok(None) if up-to-date.
    pub async fn check(&self) -> Result<Option<UpdateAvailable>> {
        debug!("checking releases for {}/{}", self.owner, self.repo);

        let url = format!(
            "https://api.github.com/repos/{}/{}/releases/latest",
            self.owner, self.repo
        );

        // Try cache first (max age 1 day)
        let cache_age = Duration::from_secs(60 * 60 * 24);
        if let Some(cache_path) = self.cache_file_path() {
            if cache_path.exists() && self.is_cache_fresh(&cache_path, cache_age) {
                if let Ok(text) = fs::read_to_string(&cache_path) {
                    if let Ok(v) = serde_json::from_str::<Value>(&text) {
                        debug!("used cached release JSON from {:?}", cache_path);
                        return Self::parse_release_json(self, v).await;
                    }
                }
            }
        }

        debug!("request url={}", url);

        // Build client with a 2-second timeout
        let client = reqwest::Client::builder()
            .timeout(Duration::from_secs(2))
            .build()
            .map_err(|e| anyhow::anyhow!(e.to_string()))?;

        let mut req = client
            .get(&url)
            .header("User-Agent", format!("{}-updater", self.bin_name));

        // Add Authorization header if token present
        if let Ok(token) = env::var("WAKE_GITHUB_TOKEN").or_else(|_| env::var("GITHUB_TOKEN")) {
            if !token.trim().is_empty() {
                req = req.bearer_auth(token.trim().to_string());
            }
        }

        // Single request, no retries. Any non-success status or network error returns Err.
        let resp = req.send().await.map_err(|e| anyhow::anyhow!(e.to_string()))?;

        let status = resp.status();
        debug!("http status={}", status);

        if !status.is_success() {
            // Do not attempt retries or fallbacks; return an error for the caller to handle
            return Err(anyhow::anyhow!(format!(
                "GitHub API returned HTTP {} when checking releases",
                status
            )));
        }

        let text = resp.text().await.map_err(|e| anyhow::anyhow!(e.to_string()))?;
        let v: Value = serde_json::from_str(&text).map_err(|e| anyhow::anyhow!(e.to_string()))?;

        // write cache (best-effort)
        if let Some(cache_path) = self.cache_file_path() {
            let _ = fs::write(&cache_path, &text);
        }

        // parse release JSON and return
        Self::parse_release_json(self, v).await
    }

    // Helper to parse Value from cache or network and build UpdateAvailable; kept async to match check() signature
    async fn parse_release_json(&self, v: Value) -> Result<Option<UpdateAvailable>> {
        let tag = v
            .get("tag_name")
            .and_then(|t| t.as_str())
            .map(|s| s.to_string());

        debug!("parsed tag={:?}", tag);

        let body = v.get("body").and_then(|b| b.as_str()).map(|s| s.to_string());

        // Prefer html_url (release page) if present
        let html_url = v
            .get("html_url")
            .and_then(|u| u.as_str())
            .map(|s| s.to_string())
            .or_else(|| Some(format!("https://github.com/{}/{}/releases", self.owner, self.repo)));

        if let Some(tag_name) = tag {
            // Normalize tags like "v1.2.3" -> "1.2.3" for comparison
            let normalize = |s: &str| s.trim().trim_start_matches('v').to_string();
            let latest = normalize(&tag_name);
            let current = normalize(&self.current_version);

            debug!("latest(normalized)={} current(normalized)={}", latest, current);

            if latest != current {
                info!("update available: {} -> {}", current, latest);
                return Ok(Some(UpdateAvailable {
                    version: tag_name,
                    body,
                    url: html_url,
                }));
            } else {
                debug!("no update available; current={}", current);
                return Ok(None);
            }
        }

        // If no tag_name present, treat as no update
        debug!("no tag_name found in release JSON; assuming no update");
        Ok(None)
    }

    /// Perform an update check and print the tag, body and release page to stdout.
    /// This centralizes all printing inside the update manager so callers (e.g. main) don't print details.
    pub async fn check_and_print(&self) -> Result<()> {
        match self.check().await {
            Ok(Some(info)) => {
                info!("Tag: {}", info.version);
                debug!("Body:\n{}", info.body.as_deref().unwrap_or("(no release body)"));
                if let Some(url) = info.url {
                    info!("Release page: {}", url);
                }
            }
            Ok(None) => {
                info!("No update available.");
            }
            Err(e) => {
                return Err(e);
            }
        }
        Ok(())
    }

    /// Perform a concise update availability report: prints "Update available: <tag>" or "No update available.".
    /// This method uses the existing `check()` method internally.
    pub async fn report_availability(&self) -> Result<()> {
        match self.check().await {
            Ok(Some(info)) => {
                info!("Update available: {}", info.version);
            }
            Ok(None) => {
                info!("No update available.");
            }
            Err(e) => {
                return Err(e);
            }
        }
        Ok(())
    }

    /// Return the Homebrew command to upgrade this tool (owner/repo/bin)
    pub fn brew_update_command(&self) -> String {
        format!("brew upgrade {}/{}/{}", self.owner, self.repo, self.bin_name)
    }

    /// Classify the update level comparing the latest tag to the current version.
    /// Rules (simple):
    /// - major: latest major > current major (e.g., current 0.8.8 -> latest 1.x.x)
    /// - minor: same major, latest minor > current minor (e.g., 0.8.8 -> 0.9.9)
    /// - patch: same major/minor, latest patch > current patch (e.g., 0.8.8 -> 0.8.9)
    /// - none: versions equal or not greater
    /// - unknown: parsing failed
    fn classify_version(&self, latest_tag: &str) -> UpdateLevel {
        // Helper to parse up to three numeric components, ignoring a leading 'v'.
        fn parse_ver(s: &str) -> Option<(u64, u64, u64)> {
            let s = s.trim().trim_start_matches('v');
            let parts: Vec<&str> = s.split('.').collect();
            let mut nums = [0u64; 3];
            for i in 0..3 {
                if let Some(p) = parts.get(i) {
                    // stop at first non-numeric suffix (e.g., 1.2.3-beta)
                    let num_str: String = p.chars().take_while(|c| c.is_ascii_digit()).collect();
                    if num_str.is_empty() {
                        return None;
                    }
                    if let Ok(n) = num_str.parse::<u64>() {
                        nums[i] = n;
                    } else {
                        return None;
                    }
                } else {
                    nums[i] = 0;
                }
            }
            Some((nums[0], nums[1], nums[2]))
        }

        let latest_opt = parse_ver(latest_tag);
        let current_opt = parse_ver(&self.current_version);
        if latest_opt.is_none() || current_opt.is_none() {
            return UpdateLevel::Unknown;
        }
        let (lm, ln, lp) = latest_opt.unwrap();
        let (cm, cn, cp) = current_opt.unwrap();

        if lm > cm {
            UpdateLevel::Major
        } else if lm == cm && ln > cn {
            UpdateLevel::Minor
        } else if lm == cm && ln == cn && lp > cp {
            UpdateLevel::Patch
        } else {
            UpdateLevel::None
        }
    }

    /// Check and return the level of the update along with the UpdateAvailable info.
    /// Returns Ok((opt_update, level)) where opt_update is Some when an update is present,
    /// and `level` indicates severity (UpdateLevel::None when no update).
    pub async fn check_with_level(&self) -> Result<(Option<UpdateAvailable>, UpdateLevel)> {
        match self.check().await {
            Ok(Some(info)) => {
                let level = self.classify_version(&info.version);
                Ok((Some(info), level))
            }
            Ok(None) => Ok((None, UpdateLevel::None)),
            Err(e) => Err(e),
        }
    }
}
