/*
*   Copyright (c) 2026. caoccao.com Sam Cao
*   All rights reserved.

*   Licensed under the Apache License, Version 2.0 (the "License");
*   you may not use this file except in compliance with the License.
*   You may obtain a copy of the License at

*   http://www.apache.org/licenses/LICENSE-2.0

*   Unless required by applicable law or agreed to in writing, software
*   distributed under the License is distributed on an "AS IS" BASIS,
*   WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
*   See the License for the specific language governing permissions and
*   limitations under the License.
*/

//! GitHub release check and the startup update policy.
//!
//! The app version is passed in by the shell (`env!("CARGO_PKG_VERSION")` of
//! the binary crate), so this crate's own version never leaks into the check.

use std::cmp::Ordering;
use std::time::Duration;

use anyhow::Result;

use crate::config::{Config, ConfigUpdate, UpdateCheckInterval};
use crate::constants::APP_NAME;
use crate::protocol::UpdateCheckResult;

/// The newest release that is neither a draft nor a prerelease.
const LATEST_RELEASE_URL: &str = "https://api.github.com/repos/caoccao/BatchMkvExtract/releases/latest";
/// The whole request, connecting included, ends after this long.
const REQUEST_TIMEOUT: Duration = Duration::from_secs(15);

/// What the shell does about updates at startup.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StartupUpdateCheck {
  /// No check yet, or the last one is older than the configured interval:
  /// run [`check_for_updates`] on a background thread and record the result
  /// with [`record_check`].
  CheckNow,
  /// A recent check exists; its cached answer is the result.
  Cached(UpdateCheckResult),
}

/// Ask GitHub for the latest stable release and compare it with
/// `app_version`. Blocking network call, at most [`REQUEST_TIMEOUT`] long.
pub fn check_for_updates(app_version: &str) -> Result<UpdateCheckResult> {
  check_for_updates_at(LATEST_RELEASE_URL, app_version)
}

/// [`check_for_updates`] against the release object at `url`.
fn check_for_updates_at(url: &str, app_version: &str) -> Result<UpdateCheckResult> {
  log::info!("Checking for updates. Current version: {}", app_version);
  let agent = ureq::AgentBuilder::new().timeout(REQUEST_TIMEOUT).build();
  let resp = agent
    .get(url)
    .set("User-Agent", APP_NAME)
    .call()
    .map_err(|e| anyhow::anyhow!("Failed to fetch the latest release: {}", e))?;
  let json: serde_json::Value = resp
    .into_json()
    .map_err(|e| anyhow::anyhow!("Failed to parse the latest release: {}", e))?;
  Ok(update_from_release(&json, app_version))
}

/// The result for the release object `release` (GitHub's `/releases/latest`).
fn update_from_release(release: &serde_json::Value, app_version: &str) -> UpdateCheckResult {
  let tag = release["tag_name"].as_str().unwrap_or("");
  log::info!("Latest release tag: {}", tag);
  if is_newer_version(tag, app_version) {
    UpdateCheckResult {
      has_update: true,
      latest_version: Some(tag.trim_start_matches(['v', 'V']).to_owned()),
    }
  } else {
    no_update()
  }
}

/// Seconds since the Unix epoch, or 0 if the clock is before it.
pub fn current_timestamp() -> i64 {
  std::time::SystemTime::now()
    .duration_since(std::time::UNIX_EPOCH)
    .map(|duration| duration.as_secs() as i64)
    .unwrap_or(0)
}

/// Length of a check interval in seconds.
pub fn interval_seconds(interval: &UpdateCheckInterval) -> i64 {
  match interval {
    UpdateCheckInterval::Daily => 86_400,
    UpdateCheckInterval::Weekly => 604_800,
    UpdateCheckInterval::Monthly => 2_592_000,
  }
}

/// A version as tags spell it: the numeric dotted parts, and whether a
/// prerelease (`-rc1`) follows them.
struct Version {
  parts: Vec<u64>,
  prerelease: bool,
}

impl Version {
  /// Anything before the first digit is a prefix (`v1.2.0`,
  /// `release-1.2.0`); the parts end at the first `-` (a prerelease) or `+`
  /// (build metadata); a part that is not a number ends them too, so a later
  /// number is never shifted into its place (`1.x.3` is `1`).
  fn parse(tag: &str) -> Self {
    let start = tag.find(|c: char| c.is_ascii_digit()).unwrap_or(tag.len());
    let rest = &tag[start..];
    let end = rest.find(['-', '+']).unwrap_or(rest.len());
    let parts = rest[..end]
      .split('.')
      .map_while(|part| part.parse::<u64>().ok())
      .collect();
    Self {
      parts,
      prerelease: rest[end..].starts_with('-'),
    }
  }

  fn cmp(&self, other: &Self) -> Ordering {
    let len = self.parts.len().max(other.parts.len());
    (0..len)
      .map(|i| {
        let part = |version: &Self| version.parts.get(i).copied().unwrap_or(0);
        part(self).cmp(&part(other))
      })
      .find(|ordering| ordering.is_ne())
      // The same numbers: a release is newer than its prereleases.
      .unwrap_or_else(|| other.prerelease.cmp(&self.prerelease))
  }
}

/// Whether the tag `latest` names a newer version than `current`: numeric
/// dotted parts (missing ones count as 0) after any prefix such as `v`, and
/// a release is newer than a prerelease of the same numbers ([`Version`]).
pub fn is_newer_version(latest: &str, current: &str) -> bool {
  let latest = Version::parse(latest);
  !latest.parts.is_empty() && latest.cmp(&Version::parse(current)) == Ordering::Greater
}

/// Records a finished check in `update` (`lastChecked` = `now`, and
/// `lastVersion` when a newer release was found) and returns what to show:
/// the update, unless it is the version the user skipped.
pub fn record_check(update: &mut ConfigUpdate, result: UpdateCheckResult, now: i64) -> UpdateCheckResult {
  update.last_checked = now;
  if let Some(ref version) = result.latest_version {
    update.last_version = version.clone();
  }
  without_ignored_version(result, &update.ignore_version)
}

/// Startup policy: check now when the interval has elapsed, otherwise reuse
/// the version found by the last check. `now` is [`current_timestamp`].
pub fn startup_check(config: &Config, now: i64, app_version: &str) -> StartupUpdateCheck {
  let update = &config.update;
  // A check "in the future" was recorded under a wrong clock: check again.
  if update.last_checked == 0
    || update.last_checked > now
    || now - update.last_checked > interval_seconds(&update.check_interval)
  {
    StartupUpdateCheck::CheckNow
  } else if !update.last_version.is_empty()
    && is_newer_version(&update.last_version, app_version)
    && update.last_version != update.ignore_version
  {
    StartupUpdateCheck::Cached(UpdateCheckResult {
      has_update: true,
      latest_version: Some(update.last_version.clone()),
    })
  } else {
    StartupUpdateCheck::Cached(no_update())
  }
}

fn no_update() -> UpdateCheckResult {
  UpdateCheckResult {
    has_update: false,
    latest_version: None,
  }
}

fn without_ignored_version(result: UpdateCheckResult, ignore_version: &str) -> UpdateCheckResult {
  if result.has_update && !ignore_version.is_empty() && result.latest_version.as_deref() == Some(ignore_version) {
    no_update()
  } else {
    result
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  const NOW: i64 = 1_800_000_000;

  fn config_checked(seconds_ago: i64, last_version: &str, ignore_version: &str) -> Config {
    let mut config = Config::default();
    config.update.check_interval = UpdateCheckInterval::Weekly;
    config.update.last_checked = NOW - seconds_ago;
    config.update.last_version = last_version.to_owned();
    config.update.ignore_version = ignore_version.to_owned();
    config
  }

  fn found(version: &str) -> UpdateCheckResult {
    UpdateCheckResult {
      has_update: true,
      latest_version: Some(version.to_owned()),
    }
  }

  #[test]
  fn newer_version_compares_numerically_and_ignores_v_prefix() {
    assert!(is_newer_version("v0.3.0", "0.2.0"));
    assert!(is_newer_version("0.10.0", "0.9.9"));
    assert!(is_newer_version("1.0", "0.9.9"));
    assert!(is_newer_version("0.2.0.1", "0.2.0"));
    assert!(!is_newer_version("0.2.0", "v0.2.0"));
    assert!(!is_newer_version("0.2", "0.2.0"));
    assert!(!is_newer_version("0.1.9", "0.2.0"));
    assert!(!is_newer_version("", "0.2.0"));
  }

  #[test]
  fn prefixes_prereleases_and_build_metadata_do_not_shift_the_parts() {
    assert!(is_newer_version("1.10.0-beta", "1.9.0"));
    assert!(!is_newer_version("1.2.0-rc1", "1.2.0"));
    assert!(is_newer_version("1.2.0", "1.2.0-rc1"));
    assert!(!is_newer_version("1.2.0+build.7", "1.2.0"));
    assert!(is_newer_version("release-1.2.0", "1.1.9"));
    assert!(!is_newer_version("1.x.3", "1.2.0"), "1.x.3 is 1, not 1.3");
    assert!(!is_newer_version("nightly", "1.0.0"));
  }

  #[test]
  fn a_release_object_gives_its_tag_without_the_prefix() {
    let release = serde_json::json!({ "tag_name": "v1.1.0", "prerelease": false });
    assert_eq!(update_from_release(&release, "1.0.0"), found("1.1.0"));
    assert_eq!(update_from_release(&release, "1.1.0"), no_update());
    assert_eq!(update_from_release(&serde_json::json!({}), "1.0.0"), no_update());
  }

  #[test]
  fn a_check_recorded_in_the_future_is_due_again() {
    let config = config_checked(-3_600, "0.3.0", "");
    assert_eq!(startup_check(&config, NOW, "0.2.0"), StartupUpdateCheck::CheckNow);
  }

  #[test]
  fn interval_lengths_are_one_day_week_and_thirty_days() {
    assert_eq!(interval_seconds(&UpdateCheckInterval::Daily), 86_400);
    assert_eq!(interval_seconds(&UpdateCheckInterval::Weekly), 7 * 86_400);
    assert_eq!(interval_seconds(&UpdateCheckInterval::Monthly), 30 * 86_400);
  }

  #[test]
  fn startup_checks_when_never_checked() {
    let config = Config::default();
    assert_eq!(config.update.last_checked, 0);
    assert_eq!(startup_check(&config, NOW, "0.2.0"), StartupUpdateCheck::CheckNow);
  }

  #[test]
  fn startup_checks_when_interval_elapsed() {
    let config = config_checked(604_801, "0.3.0", "");
    assert_eq!(startup_check(&config, NOW, "0.2.0"), StartupUpdateCheck::CheckNow);
  }

  #[test]
  fn startup_reuses_newer_cached_version_within_interval() {
    let config = config_checked(604_800, "0.3.0", "");
    assert_eq!(
      startup_check(&config, NOW, "0.2.0"),
      StartupUpdateCheck::Cached(found("0.3.0"))
    );
  }

  #[test]
  fn startup_hides_cached_version_the_user_skipped() {
    let config = config_checked(60, "0.3.0", "0.3.0");
    assert_eq!(
      startup_check(&config, NOW, "0.2.0"),
      StartupUpdateCheck::Cached(no_update())
    );
  }

  #[test]
  fn startup_reports_no_update_when_cached_version_is_not_newer() {
    assert_eq!(
      startup_check(&config_checked(60, "0.2.0", ""), NOW, "0.2.0"),
      StartupUpdateCheck::Cached(no_update())
    );
    assert_eq!(
      startup_check(&config_checked(60, "", ""), NOW, "0.2.0"),
      StartupUpdateCheck::Cached(no_update())
    );
  }

  #[test]
  fn skipped_version_is_filtered_from_a_fresh_result() {
    assert_eq!(without_ignored_version(found("0.3.0"), "0.3.0"), no_update());
    assert_eq!(without_ignored_version(found("0.3.0"), "0.2.5"), found("0.3.0"));
    assert_eq!(without_ignored_version(found("0.3.0"), ""), found("0.3.0"));
    assert_eq!(without_ignored_version(no_update(), "0.3.0"), no_update());
  }

  /// Serves one HTTP response with `status` and `body` on a local port and
  /// returns the URL to ask, and the request line and headers it received.
  fn serve_once(status: &'static str, body: &'static str) -> (String, std::thread::JoinHandle<String>) {
    use std::io::{BufRead, BufReader, Write};
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let url = format!("http://{}/releases/latest", listener.local_addr().unwrap());
    let server = std::thread::spawn(move || {
      let (stream, _) = listener.accept().unwrap();
      let mut reader = BufReader::new(stream);
      let mut request = String::new();
      loop {
        let mut line = String::new();
        if reader.read_line(&mut line).unwrap() == 0 || line == "\r\n" {
          break;
        }
        request.push_str(&line);
      }
      let mut stream = reader.into_inner();
      write!(
        stream,
        "HTTP/1.1 {status}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
        body.len()
      )
      .unwrap();
      request
    });
    (url, server)
  }

  #[test]
  fn a_check_reads_the_release_tag_from_the_server() {
    let (url, server) = serve_once("200 OK", r#"{"tag_name": "v1.2.0", "prerelease": false}"#);
    assert_eq!(check_for_updates_at(&url, "1.0.0").unwrap(), found("1.2.0"));
    let request = server.join().unwrap();
    assert!(request.starts_with("GET /releases/latest "), "{request}");
    assert!(
      request.to_ascii_lowercase().contains("user-agent: batchmkvextract"),
      "{request}"
    );
  }

  #[test]
  fn a_failed_or_unreadable_answer_is_an_error() {
    let (url, server) = serve_once("404 Not Found", r#"{"message": "Not Found"}"#);
    let error = check_for_updates_at(&url, "1.0.0").unwrap_err();
    assert!(error.to_string().contains("Failed to fetch"), "{error}");
    server.join().unwrap();
    let (url, server) = serve_once("200 OK", "not json");
    let error = check_for_updates_at(&url, "1.0.0").unwrap_err();
    assert!(error.to_string().contains("Failed to parse"), "{error}");
    server.join().unwrap();
  }

  #[test]
  fn the_release_url_names_this_repository() {
    assert_eq!(
      LATEST_RELEASE_URL,
      "https://api.github.com/repos/caoccao/BatchMkvExtract/releases/latest"
    );
  }

  #[test]
  fn current_timestamp_is_after_2020() {
    assert!(current_timestamp() > 1_577_836_800);
  }

  #[test]
  fn a_recorded_check_stores_the_time_and_version_and_hides_a_skipped_one() {
    let mut update = ConfigUpdate::default();
    assert_eq!(record_check(&mut update, found("0.3.0"), NOW), found("0.3.0"));
    assert_eq!((update.last_checked, update.last_version.as_str()), (NOW, "0.3.0"));
    update.ignore_version = "0.3.1".to_owned();
    assert_eq!(record_check(&mut update, found("0.3.1"), NOW + 1), no_update());
    assert_eq!((update.last_checked, update.last_version.as_str()), (NOW + 1, "0.3.1"));
    assert_eq!(record_check(&mut update, no_update(), NOW + 2), no_update());
    assert_eq!((update.last_checked, update.last_version.as_str()), (NOW + 2, "0.3.1"));
  }
}
