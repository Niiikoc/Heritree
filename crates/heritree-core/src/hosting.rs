//! Pull requests on GitHub, GitLab, Bitbucket and Azure DevOps.
//!
//! On GitHub and GitLab Heritree creates the pull request itself, signing in
//! with the credentials git already uses for that site (Git Credential
//! Manager, the macOS keychain, a stored token). Everywhere else, or when no
//! credentials work, it hands back the site's "new pull request" page with the
//! branches, title and description filled in, for the browser.

use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use ureq::tls::{RootCerts, TlsConfig};

use crate::{Error, Repository, Result, git_cli};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum HostKind {
    GitHub,
    GitLab,
    Bitbucket,
    AzureDevOps,
    Other,
}

/// A remote's repository on the site that hosts it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HostedRepo {
    pub kind: HostKind,
    /// `github.com`, `gitlab.example.com`, …
    pub host: String,
    /// `owner/repo`; `group/subgroup/repo` on GitLab; `org/project/_git/repo` on Azure DevOps.
    pub path: String,
}

impl HostedRepo {
    /// Reads a remote URL: `https://github.com/o/r.git`, `git@github.com:o/r.git`,
    /// `ssh://git@host:2222/o/r`, … Local paths give `None`.
    pub fn parse(url: &str) -> Option<Self> {
        let url = url.trim();
        let (host, path) = if let Some((_, rest)) = url.split_once("://") {
            let (authority, path) = rest.split_once('/')?;
            let host = authority.rsplit_once('@').map_or(authority, |(_, h)| h);
            (host.split(':').next()?, path)
        } else {
            let (authority, path) = url.split_once(':')?;
            let host = authority.rsplit_once('@').map_or(authority, |(_, h)| h);
            // `C:\repo` is a Windows path, not host `C`.
            if host.len() < 2 || host.contains(['/', '\\']) {
                return None;
            }
            (host, path)
        };
        let path = path.trim_matches('/');
        let path = path
            .strip_suffix(".git")
            .unwrap_or(path)
            .trim_end_matches('/');
        if host.is_empty() || path.is_empty() {
            return None;
        }
        let lower = host.to_ascii_lowercase();
        let (kind, host, path) = if lower == "ssh.dev.azure.com" {
            // `v3/org/project/repo` → `org/project/_git/repo`
            let mut parts = path.splitn(4, '/').skip(1);
            let (org, project, repo) = (parts.next()?, parts.next()?, parts.next()?);
            (
                HostKind::AzureDevOps,
                "dev.azure.com".to_owned(),
                format!("{org}/{project}/_git/{repo}"),
            )
        } else if lower == "dev.azure.com" || lower.ends_with(".visualstudio.com") {
            (HostKind::AzureDevOps, lower, path.to_owned())
        } else if lower.contains("github") {
            // SSH aliases such as `github.com-work` and `ssh.github.com` are still github.com.
            let host = if lower.contains("github.com") {
                "github.com".to_owned()
            } else {
                lower
            };
            (HostKind::GitHub, host, path.to_owned())
        } else if lower.contains("gitlab") {
            let host = if lower.contains("gitlab.com") {
                "gitlab.com".to_owned()
            } else {
                lower
            };
            (HostKind::GitLab, host, path.to_owned())
        } else if lower.contains("bitbucket.org") {
            (
                HostKind::Bitbucket,
                "bitbucket.org".to_owned(),
                path.to_owned(),
            )
        } else {
            (HostKind::Other, lower, path.to_owned())
        };
        Some(Self { kind, host, path })
    }

    pub fn web_url(&self) -> String {
        format!("https://{}/{}", self.host, self.path)
    }

    fn owner(&self) -> &str {
        self.path.split('/').next().unwrap_or(&self.path)
    }

    fn api_base(&self) -> Option<String> {
        match self.kind {
            HostKind::GitHub if self.host == "github.com" => Some("https://api.github.com".into()),
            HostKind::GitHub => Some(format!("https://{}/api/v3", self.host)),
            HostKind::GitLab => Some(format!("https://{}/api/v4", self.host)),
            _ => None,
        }
    }

    /// The site's page for starting a pull request, filled in as far as the site allows.
    /// `head_owner` is set when the branch lives in a fork.
    pub fn new_pull_request_url(
        &self,
        head_owner: Option<&str>,
        branch: &str,
        base: &str,
        title: &str,
        body: &str,
    ) -> Option<String> {
        let web = self.web_url();
        let (b, base_enc) = (encode(branch), encode(base));
        Some(match self.kind {
            HostKind::GitHub => {
                let head = match head_owner {
                    Some(owner) => format!("{}:{}", encode(owner), encode_path(branch)),
                    None => encode_path(branch),
                };
                format!(
                    "{web}/compare/{}...{head}?expand=1&title={}&body={}",
                    encode_path(base),
                    encode(title),
                    encode(body)
                )
            }
            HostKind::GitLab => format!(
                "{web}/-/merge_requests/new?merge_request%5Bsource_branch%5D={b}\
                 &merge_request%5Btarget_branch%5D={base_enc}\
                 &merge_request%5Btitle%5D={}&merge_request%5Bdescription%5D={}",
                encode(title),
                encode(body)
            ),
            HostKind::Bitbucket => format!("{web}/pull-requests/new?source={b}&dest={base_enc}"),
            HostKind::AzureDevOps => {
                format!("{web}/pullrequestcreate?sourceRef={b}&targetRef={base_enc}")
            }
            HostKind::Other => return None,
        })
    }
}

/// Everything the "Create pull request" dialog starts with.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PullRequestDraft {
    pub branch: String,
    /// The remote the branch is pushed to.
    pub remote: String,
    /// The remote whose repository receives the pull request: `upstream` when
    /// the branch lives in a fork, otherwise the same as `remote`.
    pub target_remote: String,
    pub host: HostKind,
    /// The receiving repository's web page.
    pub web_url: String,
    /// Branches of the receiving repository the pull request can go into.
    pub bases: Vec<String>,
    /// The branch this one was created from when known, else the default branch.
    pub base: Option<String>,
    pub title: String,
    pub body: String,
    /// False until the branch has been pushed once.
    pub on_remote: bool,
    /// Local commits the remote does not have yet. They are pushed first.
    pub unpushed: u32,
    /// True when Heritree creates the pull request itself; false when the
    /// browser finishes it.
    pub via_api: bool,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NewPullRequest {
    pub branch: String,
    pub base: String,
    pub title: String,
    pub body: String,
    #[serde(default)]
    pub draft: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum PullRequestOutcome {
    Created,
    /// One was already open for this branch; `url` points to it.
    AlreadyOpen,
    /// Heritree could not create it; open `url` to finish in the browser.
    OpenInBrowser,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PullRequest {
    pub url: String,
    pub number: Option<u64>,
    pub outcome: PullRequestOutcome,
}

impl Repository {
    /// Works out where a pull request for `branch` (default: the current
    /// branch) would go, and drafts its title and description from the commits.
    pub fn pull_request_draft(&self, branch: Option<&str>) -> Result<PullRequestDraft> {
        let branch = match branch {
            Some(b) => b.to_owned(),
            None => self
                .current_branch()
                .ok_or_else(|| Error::Git("check out a branch to open a pull request".into()))?,
        };
        let remote = self.push_remote(&branch)?;
        let target_remote = self.target_remote(&remote);
        let target = self.hosted(&target_remote)?;
        if target.kind == HostKind::Other {
            return Err(Error::Hosting(format!(
                "Heritree can open pull requests on GitHub, GitLab, Bitbucket and Azure DevOps, \
                 but {target_remote} is on {}",
                target.host
            )));
        }

        let bases: Vec<String> = self
            .remote_branches(&target_remote)?
            .into_iter()
            .filter(|b| !(b == &branch && target_remote == remote))
            .collect();
        let parent = self
            .lineage()
            .ok()
            .and_then(|all| all.into_iter().find(|l| l.name == branch)?.parent);
        let base = parent
            .filter(|p| bases.contains(p))
            .or_else(|| {
                self.remote_head(&target_remote)
                    .filter(|h| bases.contains(h))
            })
            .or_else(|| {
                ["main", "master", "develop", "trunk"]
                    .into_iter()
                    .find(|b| bases.iter().any(|x| x == b))
                    .map(str::to_owned)
            })
            .or_else(|| bases.first().cloned());

        let commits = match &base {
            Some(base) => {
                self.commits_between(&format!("refs/remotes/{target_remote}/{base}"), &branch)
            }
            None => Vec::new(),
        };
        let (title, body) = match commits.as_slice() {
            [(summary, body)] => (summary.clone(), body.clone()),
            [] => (humanize(&branch), String::new()),
            many => (
                humanize(&branch),
                many.iter()
                    .map(|(s, _)| format!("- {s}"))
                    .collect::<Vec<_>>()
                    .join("\n"),
            ),
        };

        let remote_ref = format!("refs/remotes/{remote}/{branch}");
        let on_remote = self
            .git(&["rev-parse", "--verify", "--quiet", &remote_ref])
            .is_ok();
        let unpushed = if on_remote {
            self.count(&format!("{remote_ref}..refs/heads/{branch}"))
        } else {
            commits.len() as u32
        };
        // GitLab merge requests between forks need project ids; the browser handles those.
        let via_api = match target.kind {
            HostKind::GitHub => true,
            HostKind::GitLab => target_remote == remote,
            _ => false,
        };

        Ok(PullRequestDraft {
            branch,
            remote,
            target_remote,
            host: target.kind,
            web_url: target.web_url(),
            bases,
            base,
            title,
            body,
            on_remote,
            unpushed,
            via_api,
        })
    }

    /// Pushes the branch if the remote is missing commits, then opens the pull
    /// request, or returns the browser page that finishes it.
    pub fn create_pull_request(&self, pr: &NewPullRequest) -> Result<PullRequest> {
        let draft = self.pull_request_draft(Some(&pr.branch))?;
        if !draft.on_remote || draft.unpushed > 0 {
            self.push_branch(&pr.branch, &draft.remote)?;
        }
        let target = self.hosted(&draft.target_remote)?;
        let head_owner = if draft.target_remote == draft.remote {
            None
        } else {
            Some(self.hosted(&draft.remote)?.owner().to_owned())
        };
        let in_browser = || {
            target
                .new_pull_request_url(
                    head_owner.as_deref(),
                    &pr.branch,
                    &pr.base,
                    &pr.title,
                    &pr.body,
                )
                .map(|url| PullRequest {
                    url,
                    number: None,
                    outcome: PullRequestOutcome::OpenInBrowser,
                })
                .ok_or_else(|| Error::Hosting(format!("no pull request page for {}", target.host)))
        };

        let (true, Some(api)) = (draft.via_api, target.api_base()) else {
            return in_browser();
        };
        let Some(token) = self.credential_token(&target.host) else {
            return in_browser();
        };
        let client = Client {
            dir: self.dir(),
            api: &api,
            token: &token,
            kind: target.kind,
        };
        let result = match target.kind {
            HostKind::GitHub => client.github(&target, head_owner.as_deref(), pr),
            _ => client.gitlab(&target, pr),
        };
        match result {
            Ok(created) => Ok(created),
            // Wrong or missing scopes: the browser uses the user's own session.
            Err(ApiError::NotAllowed) => in_browser(),
            Err(ApiError::Failed(message)) => Err(Error::Hosting(message)),
        }
    }

    fn push_remote(&self, branch: &str) -> Result<String> {
        match self.git(&["config", "--get", &format!("branch.{branch}.pushRemote")]) {
            Ok(remote) => Ok(remote.trim().to_owned()),
            Err(_) => self.default_remote(),
        }
    }

    /// `upstream` when the branch's remote is a GitHub fork of it, as `gh` assumes.
    fn target_remote(&self, remote: &str) -> String {
        let fork_of = |upstream: &str| -> Option<()> {
            let (ours, theirs) = (self.hosted(remote).ok()?, self.hosted(upstream).ok()?);
            (ours.kind == HostKind::GitHub && ours.host == theirs.host && ours.path != theirs.path)
                .then_some(())
        };
        let upstream = "upstream";
        let has_upstream = self
            .remotes()
            .unwrap_or_default()
            .iter()
            .any(|r| r == upstream);
        if remote != upstream && has_upstream && fork_of(upstream).is_some() {
            upstream.to_owned()
        } else {
            remote.to_owned()
        }
    }

    fn hosted(&self, remote: &str) -> Result<HostedRepo> {
        let url = self.git(&["remote", "get-url", remote])?;
        HostedRepo::parse(&url).ok_or_else(|| {
            Error::Hosting(format!(
                "{remote} points to {}, which is not on a hosting site",
                url.trim()
            ))
        })
    }

    fn remote_branches(&self, remote: &str) -> Result<Vec<String>> {
        let prefix = format!("refs/remotes/{remote}/");
        Ok(self
            .git(&["for-each-ref", "--format=%(refname)", &prefix])?
            .lines()
            .filter_map(|r| r.strip_prefix(&prefix))
            .filter(|b| *b != "HEAD")
            .map(str::to_owned)
            .collect())
    }

    /// The remote's default branch, as recorded by clone or `git remote set-head`.
    fn remote_head(&self, remote: &str) -> Option<String> {
        let head = self
            .git(&[
                "symbolic-ref",
                "--quiet",
                &format!("refs/remotes/{remote}/HEAD"),
            ])
            .ok()?;
        head.trim()
            .strip_prefix(&format!("refs/remotes/{remote}/"))
            .map(str::to_owned)
    }

    /// (summary, body) of the commits in `branch` but not `base`, oldest first.
    fn commits_between(&self, base: &str, branch: &str) -> Vec<(String, String)> {
        let range = format!("{base}..refs/heads/{branch}");
        self.git(&[
            "log",
            "--reverse",
            "--no-merges",
            "--format=%s%x1f%b%x1e",
            &range,
        ])
        .unwrap_or_default()
        .split('\u{1e}')
        .filter_map(|entry| {
            let (summary, body) = entry.trim_start_matches('\n').split_once('\u{1f}')?;
            Some((summary.to_owned(), body.trim().to_owned()))
        })
        .collect()
    }

    fn count(&self, range: &str) -> u32 {
        self.git(&["rev-list", "--count", range])
            .ok()
            .and_then(|n| n.trim().parse().ok())
            .unwrap_or(0)
    }

    fn push_branch(&self, branch: &str, remote: &str) -> Result<()> {
        let tracked = self
            .git(&[
                "rev-parse",
                "--abbrev-ref",
                &format!("{branch}@{{upstream}}"),
            ])
            .is_ok();
        let spec = format!("refs/heads/{branch}:refs/heads/{branch}");
        let mut args = vec!["push"];
        if !tracked {
            args.push("-u");
        }
        args.extend([remote, spec.as_str()]);
        self.exec(&args).map(drop)
    }

    /// The token or password git would use for `https://<host>`. Asking may
    /// show the credential manager's sign-in window. Never logged.
    fn credential_token(&self, host: &str) -> Option<String> {
        let input = format!("protocol=https\nhost={host}\n\n");
        let out =
            git_cli::run_with_input(self.dir(), &["credential", "fill"], input.as_bytes()).ok()?;
        String::from_utf8_lossy(&out)
            .lines()
            .find_map(|l| l.strip_prefix("password="))
            .filter(|p| !p.is_empty())
            .map(str::to_owned)
    }
}

enum ApiError {
    /// Signed out, missing scopes, or no access to the repository.
    NotAllowed,
    Failed(String),
}

struct Client<'a> {
    dir: &'a std::path::Path,
    api: &'a str,
    token: &'a str,
    kind: HostKind,
}

impl Client<'_> {
    fn github(
        &self,
        repo: &HostedRepo,
        head_owner: Option<&str>,
        pr: &NewPullRequest,
    ) -> std::result::Result<PullRequest, ApiError> {
        let head = match head_owner {
            Some(owner) => format!("{owner}:{}", pr.branch),
            None => pr.branch.clone(),
        };
        let pulls = format!("{}/repos/{}/pulls", self.api, repo.path);
        let (status, body) = self.send(
            "POST",
            &pulls,
            Some(json!({
                "title": pr.title,
                "body": pr.body,
                "head": head,
                "base": pr.base,
                "draft": pr.draft,
            })),
        )?;
        match status {
            200..=299 => Ok(PullRequest {
                url: str_field(&body, "html_url"),
                number: body["number"].as_u64(),
                outcome: PullRequestOutcome::Created,
            }),
            401 | 403 | 404 => Err(ApiError::NotAllowed),
            422 if github_message(&body).contains("already exists") => {
                let owner = head_owner.unwrap_or_else(|| repo.owner());
                let query = format!(
                    "{pulls}?state=open&head={}",
                    encode(&format!("{owner}:{}", pr.branch))
                );
                let (_, open) = self.send("GET", &query, None)?;
                let first = &open[0];
                Ok(PullRequest {
                    url: str_field(first, "html_url"),
                    number: first["number"].as_u64(),
                    outcome: PullRequestOutcome::AlreadyOpen,
                })
            }
            _ => Err(ApiError::Failed(format!(
                "GitHub: {}",
                github_message(&body)
            ))),
        }
    }

    fn gitlab(
        &self,
        repo: &HostedRepo,
        pr: &NewPullRequest,
    ) -> std::result::Result<PullRequest, ApiError> {
        let requests = format!(
            "{}/projects/{}/merge_requests",
            self.api,
            encode(&repo.path)
        );
        let title = if pr.draft && !pr.title.starts_with("Draft:") {
            format!("Draft: {}", pr.title)
        } else {
            pr.title.clone()
        };
        let (status, body) = self.send(
            "POST",
            &requests,
            Some(json!({
                "source_branch": pr.branch,
                "target_branch": pr.base,
                "title": title,
                "description": pr.body,
            })),
        )?;
        match status {
            200..=299 => Ok(PullRequest {
                url: str_field(&body, "web_url"),
                number: body["iid"].as_u64(),
                outcome: PullRequestOutcome::Created,
            }),
            401 | 403 | 404 => Err(ApiError::NotAllowed),
            409 => {
                let query = format!(
                    "{requests}?state=opened&source_branch={}&target_branch={}",
                    encode(&pr.branch),
                    encode(&pr.base)
                );
                let (_, open) = self.send("GET", &query, None)?;
                let first = &open[0];
                Ok(PullRequest {
                    url: str_field(first, "web_url"),
                    number: first["iid"].as_u64(),
                    outcome: PullRequestOutcome::AlreadyOpen,
                })
            }
            _ => {
                let message = match &body["message"] {
                    Value::String(s) => s.clone(),
                    Value::Null => format!("HTTP {status}"),
                    other => other.to_string(),
                };
                Err(ApiError::Failed(format!("GitLab: {message}")))
            }
        }
    }

    /// Sends one request and records it in the command log.
    fn send(
        &self,
        method: &str,
        url: &str,
        body: Option<Value>,
    ) -> std::result::Result<(u16, Value), ApiError> {
        let started = Instant::now();
        let agent: ureq::Agent = ureq::Agent::config_builder()
            .http_status_as_error(false)
            .user_agent("Heritree")
            .timeout_global(Some(Duration::from_secs(30)))
            .tls_config(
                TlsConfig::builder()
                    .root_certs(RootCerts::PlatformVerifier)
                    .build(),
            )
            .build()
            .into();
        let accept = match self.kind {
            HostKind::GitHub => "application/vnd.github+json",
            _ => "application/json",
        };
        let auth = format!("Bearer {}", self.token);
        let response = match body {
            Some(body) => agent
                .post(url)
                .header("Authorization", &auth)
                .header("Accept", accept)
                .send_json(body),
            None => agent
                .get(url)
                .header("Authorization", &auth)
                .header("Accept", accept)
                .call(),
        };
        let result = response
            .map_err(|e| Error::Hosting(format!("could not reach {url}: {e}")))
            .map(|mut r| {
                let status = r.status().as_u16();
                let json = r.body_mut().read_json::<Value>().unwrap_or(Value::Null);
                (status, json)
            });
        let logged = match &result {
            Ok((status, _)) if (200..300).contains(status) => Ok(()),
            Ok((status, _)) => Err(Error::Hosting(format!("HTTP {status}"))),
            Err(e) => Err(Error::Hosting(e.to_string())),
        };
        git_cli::record(self.dir, format!("{method} {url}"), &logged, started);
        result.map_err(|e| ApiError::Failed(e.to_string()))
    }
}

fn str_field(value: &Value, key: &str) -> String {
    value[key].as_str().unwrap_or_default().to_owned()
}

/// GitHub's validation details ("No commits between main and x") are in `errors`.
fn github_message(body: &Value) -> String {
    let details: Vec<&str> = body["errors"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|e| e["message"].as_str())
        .collect();
    if details.is_empty() {
        body["message"]
            .as_str()
            .unwrap_or("unexpected response")
            .to_owned()
    } else {
        details.join("; ")
    }
}

/// `feature/dark-mode` → `Dark mode`.
fn humanize(branch: &str) -> String {
    let name = branch.rsplit('/').next().unwrap_or(branch);
    let words = name.replace(['-', '_'], " ");
    let mut chars = words.trim().chars();
    match chars.next() {
        Some(first) => first.to_uppercase().chain(chars).collect(),
        None => branch.to_owned(),
    }
}

fn encode(text: &str) -> String {
    percent_encode(text, b"-._~")
}

fn encode_path(text: &str) -> String {
    percent_encode(text, b"-._~/")
}

fn percent_encode(text: &str, keep: &[u8]) -> String {
    let mut out = String::with_capacity(text.len());
    for byte in text.bytes() {
        if byte.is_ascii_alphanumeric() || keep.contains(&byte) {
            out.push(byte as char);
        } else {
            out.push_str(&format!("%{byte:02X}"));
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{BufRead, BufReader, Read, Write};
    use std::net::TcpListener;
    use std::sync::{Arc, Mutex};

    fn parsed(url: &str) -> (HostKind, String, String) {
        let r = HostedRepo::parse(url).unwrap_or_else(|| panic!("{url}"));
        (r.kind, r.host, r.path)
    }

    #[test]
    fn parses_remote_urls() {
        let gh = (
            HostKind::GitHub,
            "github.com".to_owned(),
            "nick/heritree".to_owned(),
        );
        assert_eq!(parsed("https://github.com/nick/heritree.git"), gh);
        assert_eq!(parsed("https://nick@github.com/nick/heritree"), gh);
        assert_eq!(parsed("git@github.com:nick/heritree.git"), gh);
        assert_eq!(parsed("git@github.com-work:nick/heritree.git"), gh);
        assert_eq!(parsed("ssh://git@ssh.github.com:443/nick/heritree.git"), gh);
        assert_eq!(
            parsed("git@gitlab.com:group/sub/app.git"),
            (
                HostKind::GitLab,
                "gitlab.com".into(),
                "group/sub/app".into()
            )
        );
        assert_eq!(
            parsed("https://github.acme.io/team/app"),
            (HostKind::GitHub, "github.acme.io".into(), "team/app".into())
        );
        assert_eq!(
            parsed("git@ssh.dev.azure.com:v3/acme/Web/site"),
            (
                HostKind::AzureDevOps,
                "dev.azure.com".into(),
                "acme/Web/_git/site".into()
            )
        );
        assert_eq!(
            parsed("https://acme@dev.azure.com/acme/Web/_git/site"),
            (
                HostKind::AzureDevOps,
                "dev.azure.com".into(),
                "acme/Web/_git/site".into()
            )
        );
        assert_eq!(
            parsed("git@bitbucket.org:team/app.git").0,
            HostKind::Bitbucket
        );
        assert_eq!(parsed("https://git.example.com/x/y").0, HostKind::Other);
        assert_eq!(HostedRepo::parse("C:\\src\\repo"), None);
        assert_eq!(HostedRepo::parse("/srv/git/repo.git"), None);
        assert_eq!(HostedRepo::parse("file:///srv/git/repo.git"), None);
    }

    #[test]
    fn builds_browser_urls() {
        let gh = HostedRepo::parse("git@github.com:nick/heritree.git").unwrap();
        assert_eq!(
            gh.new_pull_request_url(None, "feature/x", "main", "Add x", "- a\n- b")
                .unwrap(),
            "https://github.com/nick/heritree/compare/main...feature/x?expand=1&title=Add%20x&body=-%20a%0A-%20b"
        );
        assert_eq!(
            gh.new_pull_request_url(Some("fork"), "fix", "main", "", "")
                .unwrap(),
            "https://github.com/nick/heritree/compare/main...fork:fix?expand=1&title=&body="
        );
        let gl = HostedRepo::parse("https://gitlab.com/g/app.git").unwrap();
        assert!(
            gl.new_pull_request_url(None, "feature/x", "main", "T", "")
                .unwrap()
                .starts_with("https://gitlab.com/g/app/-/merge_requests/new?merge_request%5Bsource_branch%5D=feature%2Fx")
        );
        let other = HostedRepo::parse("https://git.example.com/x/y").unwrap();
        assert_eq!(other.new_pull_request_url(None, "a", "b", "", ""), None);
    }

    #[test]
    fn humanizes_branch_names() {
        assert_eq!(humanize("feature/dark-mode"), "Dark mode");
        assert_eq!(humanize("fix_login"), "Fix login");
        assert_eq!(humanize("main"), "Main");
    }

    /// A one-shot-per-connection HTTP server answering from `routes`
    /// (method + path prefix → status, body). Returns its base URL and the requests it saw.
    fn serve(routes: Vec<(&'static str, u16, &'static str)>) -> (String, Arc<Mutex<Vec<String>>>) {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let base = format!("http://{}", listener.local_addr().unwrap());
        let seen = Arc::new(Mutex::new(Vec::new()));
        let log = seen.clone();
        std::thread::spawn(move || {
            for stream in listener.incoming() {
                let mut stream = stream.unwrap();
                let mut reader = BufReader::new(stream.try_clone().unwrap());
                let mut head = String::new();
                let mut length = 0;
                loop {
                    let mut line = String::new();
                    reader.read_line(&mut line).unwrap();
                    if let Some(v) = line.to_ascii_lowercase().strip_prefix("content-length:") {
                        length = v.trim().parse().unwrap();
                    }
                    if line == "\r\n" || line.is_empty() {
                        break;
                    }
                    head.push_str(&line);
                }
                let mut body = vec![0; length];
                reader.read_exact(&mut body).unwrap();
                let request_line = head.lines().next().unwrap().to_owned();
                log.lock()
                    .unwrap()
                    .push(format!("{request_line} {}", String::from_utf8_lossy(&body)));
                let (status, reply) = routes
                    .iter()
                    .find(|(prefix, _, _)| request_line.starts_with(prefix))
                    .map(|(_, s, b)| (*s, *b))
                    .unwrap_or((500, "{}"));
                let _ = write!(
                    stream,
                    "HTTP/1.1 {status} X\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{reply}",
                    reply.len()
                );
            }
        });
        (base, seen)
    }

    /// The JSON body of the first request the server saw.
    fn sent_json(seen: &Mutex<Vec<String>>) -> Value {
        let first = seen.lock().unwrap()[0].clone();
        let body = &first[first.find('{').unwrap()..];
        serde_json::from_str(body).unwrap()
    }

    fn request() -> NewPullRequest {
        NewPullRequest {
            branch: "feature/x".into(),
            base: "main".into(),
            title: "Add x".into(),
            body: "Body".into(),
            draft: true,
        }
    }

    #[test]
    fn creates_github_pull_requests_and_finds_open_ones() {
        let repo = HostedRepo::parse("https://github.com/nick/heritree").unwrap();
        let dir = std::env::temp_dir();

        let (api, seen) = serve(vec![(
            "POST /repos/nick/heritree/pulls",
            201,
            r#"{"html_url":"https://github.com/nick/heritree/pull/7","number":7}"#,
        )]);
        let client = Client {
            dir: &dir,
            api: &api,
            token: "t",
            kind: HostKind::GitHub,
        };
        let Ok(pr) = client.github(&repo, None, &request()) else {
            panic!("create failed")
        };
        assert_eq!(pr.number, Some(7));
        assert_eq!(pr.outcome, PullRequestOutcome::Created);
        let sent = sent_json(&seen);
        assert_eq!(
            (&sent["head"], &sent["draft"]),
            (&json!("feature/x"), &json!(true))
        );

        let (api, _) = serve(vec![
            (
                "POST /repos/nick/heritree/pulls",
                422,
                r#"{"message":"Validation Failed","errors":[{"message":"A pull request already exists for nick:feature/x."}]}"#,
            ),
            (
                "GET /repos/nick/heritree/pulls?state=open&head=nick%3Afeature%2Fx",
                200,
                r#"[{"html_url":"https://github.com/nick/heritree/pull/3","number":3}]"#,
            ),
        ]);
        let client = Client {
            dir: &dir,
            api: &api,
            token: "t",
            kind: HostKind::GitHub,
        };
        let Ok(pr) = client.github(&repo, None, &request()) else {
            panic!("lookup failed")
        };
        assert_eq!(
            (pr.number, pr.outcome),
            (Some(3), PullRequestOutcome::AlreadyOpen)
        );

        let (api, _) = serve(vec![(
            "POST",
            422,
            r#"{"message":"Validation Failed","errors":[{"message":"No commits between main and feature/x"}]}"#,
        )]);
        let client = Client {
            dir: &dir,
            api: &api,
            token: "t",
            kind: HostKind::GitHub,
        };
        match client.github(&repo, None, &request()) {
            Err(ApiError::Failed(m)) => {
                assert_eq!(m, "GitHub: No commits between main and feature/x")
            }
            _ => panic!("expected a failure"),
        }

        let (api, _) = serve(vec![("POST", 401, r#"{"message":"Bad credentials"}"#)]);
        let client = Client {
            dir: &dir,
            api: &api,
            token: "t",
            kind: HostKind::GitHub,
        };
        assert!(matches!(
            client.github(&repo, None, &request()),
            Err(ApiError::NotAllowed)
        ));
    }

    #[test]
    fn creates_gitlab_merge_requests() {
        let repo = HostedRepo::parse("git@gitlab.com:group/app.git").unwrap();
        let dir = std::env::temp_dir();
        let (api, seen) = serve(vec![(
            "POST /projects/group%2Fapp/merge_requests",
            201,
            r#"{"web_url":"https://gitlab.com/group/app/-/merge_requests/4","iid":4}"#,
        )]);
        let client = Client {
            dir: &dir,
            api: &api,
            token: "t",
            kind: HostKind::GitLab,
        };
        let Ok(pr) = client.gitlab(&repo, &request()) else {
            panic!("create failed")
        };
        assert_eq!(pr.number, Some(4));
        assert_eq!(sent_json(&seen)["title"], "Draft: Add x");
    }
}
