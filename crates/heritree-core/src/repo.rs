use std::path::Path;

use gix::bstr::ByteSlice;
use gix::refs::Category;
use gix::revision::walk::Sorting;
use gix::traverse::commit::simple::CommitTimeOrder;

use crate::{
    CommitInfo, Error, FileStatus, HeadInfo, RefInfo, RefKind, RepoInfo, Result, git_cli, graph,
    status,
};

/// An open repository.
pub struct Repository {
    inner: gix::Repository,
}

impl Repository {
    /// Opens the repository containing `path`, searching parent folders like `git` does.
    pub fn open(path: impl AsRef<Path>) -> Result<Self> {
        let path = path.as_ref();
        let inner = gix::discover(path).map_err(|e| Error::NotARepository {
            path: path.to_owned(),
            reason: e.to_string(),
        })?;
        Ok(Self { inner })
    }

    pub fn info(&self) -> Result<RepoInfo> {
        let workdir = self.inner.workdir().map(display_path);
        let git_dir = display_path(self.inner.git_dir());
        let name = Path::new(workdir.as_deref().unwrap_or(&git_dir))
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default();

        let head = self.inner.head().map_err(Error::git)?;
        let head = HeadInfo {
            branch: head
                .referent_name()
                .map(|n| n.shorten().to_str_lossy().into_owned()),
            id: head.id().map(|id| id.to_string()),
            detached: head.is_detached(),
            unborn: head.is_unborn(),
        };

        Ok(RepoInfo {
            name,
            git_dir,
            workdir,
            head,
        })
    }

    /// Local branches, remote-tracking branches and tags, sorted by kind then name.
    pub fn refs(&self) -> Result<Vec<RefInfo>> {
        let platform = self.inner.references().map_err(Error::git)?;
        let mut refs = Vec::new();
        for reference in platform.all().map_err(Error::git)? {
            let mut reference = reference.map_err(Error::git)?;
            let name = reference.name();
            let kind = match name.category() {
                Some(Category::LocalBranch) => RefKind::LocalBranch,
                Some(Category::RemoteBranch) => RefKind::RemoteBranch,
                Some(Category::Tag) => RefKind::Tag,
                _ => continue,
            };
            let full_name = name.as_bstr().to_str_lossy().into_owned();
            let short_name = name.shorten().to_str_lossy().into_owned();
            // `origin/HEAD` only points at another remote branch.
            if kind == RefKind::RemoteBranch && short_name.ends_with("/HEAD") {
                continue;
            }
            let Ok(target) = reference.peel_to_id() else {
                continue;
            };
            refs.push(RefInfo {
                name: full_name,
                short_name,
                kind,
                target: target.to_string(),
            });
        }
        refs.sort_by(|a, b| (a.kind, &a.short_name).cmp(&(b.kind, &b.short_name)));
        Ok(refs)
    }

    /// The newest `limit` commits reachable from any branch, tag or HEAD, in
    /// graph order (children before parents).
    pub fn log(&self, limit: usize) -> Result<Vec<CommitInfo>> {
        let mut tips: Vec<gix::ObjectId> = Vec::new();
        if let Ok(head) = self.inner.head_id() {
            tips.push(head.detach());
        }
        for r in self.refs()? {
            let Ok(id) = gix::ObjectId::from_hex(r.target.as_bytes()) else {
                continue;
            };
            // Tags can point at trees or blobs; only commits start a walk.
            let is_commit = self
                .inner
                .find_object(id)
                .map(|o| o.kind == gix::object::Kind::Commit);
            if is_commit.unwrap_or(false) && !tips.contains(&id) {
                tips.push(id);
            }
        }
        if tips.is_empty() {
            return Ok(Vec::new());
        }

        let walk = self
            .inner
            .rev_walk(tips)
            .sorting(Sorting::ByCommitTime(CommitTimeOrder::NewestFirst))
            .all()
            .map_err(Error::git)?;

        let mut commits = Vec::new();
        for info in walk.take(limit) {
            let info = info.map_err(Error::git)?;
            let commit = info.object().map_err(Error::git)?;
            let author = commit.author().map_err(Error::git)?.trim();
            let id = info.id;
            commits.push(CommitInfo {
                id: id.to_string(),
                short_id: id.to_hex_with_len(7).to_string(),
                parents: info.parent_ids.iter().map(|p| p.to_string()).collect(),
                summary: commit
                    .message()
                    .map_err(Error::git)?
                    .summary()
                    .to_str_lossy()
                    .into_owned(),
                author_name: author.name.to_str_lossy().into_owned(),
                author_email: author.email.to_str_lossy().into_owned(),
                author_time: author.time().map(|t| t.seconds).unwrap_or_default(),
                commit_time: info.commit_time(),
            });
        }
        Ok(graph::topo_order(commits))
    }

    /// Changes in the index and the working tree.
    pub fn status(&self) -> Result<Vec<FileStatus>> {
        let workdir = self.inner.workdir().ok_or(Error::Bare)?;
        let output = git_cli::run(workdir, status::ARGS)?;
        status::parse_porcelain_v2(&output)
    }
}

/// Paths go to the UI as strings with the platform's own separators.
fn display_path(path: &Path) -> String {
    let path = path.canonicalize().unwrap_or_else(|_| path.to_owned());
    let text = path.display().to_string();
    // canonicalize() on Windows returns verbatim paths (`\\?\C:\...`).
    text.strip_prefix(r"\\?\")
        .map(str::to_owned)
        .unwrap_or(text)
}
