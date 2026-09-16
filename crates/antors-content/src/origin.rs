//! Where a file came from, and what that makes its "Edit this page" link.
//!
//! An origin is shared by every file collected from one start path of one ref
//! of one source, which is why it is held behind an [`Arc`] rather than copied
//! into each: a site of ten thousand files has a handful of origins.
//!
//! [`Arc`]: std::sync::Arc

use std::path::PathBuf;

/// What kind of ref a file was read from.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RefType {
    /// A branch, or the worktree checked out at one.
    Branch,

    /// A tag.
    Tag,
}

impl RefType {
    /// The name the `page-origin-reftype` attribute uses.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Branch => "branch",
            Self::Tag => "tag",
        }
    }
}

/// One source, ref and start path, shared by every file collected from it.
#[derive(Clone, Debug)]
pub struct Origin {
    /// The repository URL as the playbook wrote it.
    pub url: Option<String>,

    /// The same, in the form a browser can open — no `.git` suffix, no
    /// `user@host:path` form. `None` when the source is a local directory with
    /// no remote, which is the case a `file://` edit link exists for.
    pub web_url: Option<String>,

    /// The branch or tag name.
    pub refname: String,

    /// Whether `refname` is a branch or a tag.
    pub reftype: RefType,

    /// The directory within the repository that holds the `antora.yml`.
    pub start_path: String,

    /// The checked-out directory, when the content was read from one rather
    /// than from a git object database.
    pub worktree: Option<PathBuf>,

    /// The `edit_url` template, with `{web_url}`, `{refname}` and `{path}`
    /// placeholders. `None` switches the link off.
    pub edit_url_pattern: Option<String>,
}

impl Origin {
    /// The "Edit this page" link for a file at `relative_path` within the
    /// start path.
    ///
    /// A tag has no edit link: a tag is a fixed point in history, and offering
    /// to edit one invites a change that cannot be made. A worktree with no
    /// usable remote gets a `file://` link instead, which opens the actual file
    /// the reader is looking at — the most useful thing available when there is
    /// no forge to send them to.
    pub fn edit_url(&self, relative_path: &str) -> Option<String> {
        if self.reftype == RefType::Tag {
            return None;
        }

        let path = self.path_within_repository(relative_path);

        match (&self.edit_url_pattern, &self.web_url) {
            (Some(pattern), Some(web_url)) => Some(
                pattern
                    .replace("{web_url}", web_url)
                    .replace("{refname}", &self.refname)
                    .replace("{path}", &path),
            ),

            // No pattern means the playbook switched the link off; a pattern
            // with no web URL has nothing to put in it.
            (Some(_), None) | (None, _) => self.file_url(relative_path),
        }
    }

    /// The `file://` link to the file in the worktree, if there is one.
    fn file_url(&self, relative_path: &str) -> Option<String> {
        // Only a pattern that was never set falls back to the worktree; a
        // pattern that is set and unusable is a configuration problem rather
        // than a reason to leak a local path into a published site.
        if self.edit_url_pattern.is_some() {
            return None;
        }

        let worktree = self.worktree.as_ref()?;
        let path = worktree.join(self.path_within_repository(relative_path));

        Some(format!("file://{}", path.to_string_lossy()))
    }

    /// A file's path from the repository root rather than from the start path.
    fn path_within_repository(&self, relative_path: &str) -> String {
        if self.start_path.is_empty() {
            return relative_path.to_string();
        }

        format!("{}/{relative_path}", self.start_path)
    }
}

/// Turn a repository URL into one a browser can open.
///
/// Both `git@host:org/repo.git` and `https://host/org/repo.git` name a page a
/// reader can be sent to, but neither is that page's address. A local path
/// names no page at all.
pub fn web_url(url: &str) -> Option<String> {
    if !url.contains("://") {
        // `git@github.com:org/repo.git` — the SCP-like form, which is the only
        // other shape a remote takes.
        let (user_host, path) = url.split_once(':')?;
        let host = user_host
            .split_once('@')
            .map_or(user_host, |(_, host)| host);

        if host.is_empty() || path.is_empty() {
            return None;
        }

        return Some(format!("https://{host}/{}", trim_git(path)));
    }

    let (scheme, rest) = url.split_once("://")?;

    match scheme {
        "http" | "https" => Some(trim_git(url).to_string()),

        // `ssh://git@host/org/repo.git` and `git://host/org/repo.git` both
        // point at something a browser reaches over HTTPS.
        "ssh" | "git" => {
            let rest = rest.split_once('@').map_or(rest, |(_, rest)| rest);
            Some(format!("https://{}", trim_git(rest)))
        }

        // A `file://` URL names a place on this machine rather than a page,
        // and anything else is a scheme this does not know how to turn into
        // one.
        _ => None,
    }
}

/// Drop a trailing `.git` and any trailing slash.
fn trim_git(url: &str) -> &str {
    let url = url.trim_end_matches('/');
    url.strip_suffix(".git").unwrap_or(url)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn origin(reftype: RefType, pattern: Option<&str>, web: Option<&str>) -> Origin {
        Origin {
            url: None,
            web_url: web.map(str::to_string),
            refname: "main".to_string(),
            reftype,
            start_path: "docs".to_string(),
            worktree: Some(PathBuf::from("/repo")),
            edit_url_pattern: pattern.map(str::to_string),
        }
    }

    #[test]
    fn a_pattern_is_filled_in_from_the_start_path_upwards() {
        let origin = origin(
            RefType::Branch,
            Some("{web_url}/edit/{refname}/{path}"),
            Some("https://example.org/org/repo"),
        );

        assert_eq!(
            origin.edit_url("modules/ROOT/pages/index.adoc").as_deref(),
            Some("https://example.org/org/repo/edit/main/docs/modules/ROOT/pages/index.adoc")
        );
    }

    #[test]
    fn a_tag_has_no_edit_link() {
        let origin = origin(
            RefType::Tag,
            Some("{web_url}/edit/{refname}/{path}"),
            Some("https://example.org/org/repo"),
        );

        assert_eq!(origin.edit_url("modules/ROOT/pages/index.adoc"), None);
    }

    #[test]
    fn a_worktree_with_no_remote_links_to_the_file() {
        let origin = origin(RefType::Branch, None, None);

        assert_eq!(
            origin.edit_url("modules/ROOT/pages/index.adoc").as_deref(),
            Some("file:///repo/docs/modules/ROOT/pages/index.adoc")
        );
    }

    #[test]
    fn a_pattern_that_cannot_be_filled_in_publishes_nothing() {
        // The playbook asked for a forge link and there is no forge. A
        // `file://` path to the machine that ran the build is not an answer.
        let origin = origin(RefType::Branch, Some("{web_url}/edit/{path}"), None);

        assert_eq!(origin.edit_url("modules/ROOT/pages/index.adoc"), None);
    }

    #[test]
    fn web_urls_are_derived_from_every_remote_form() {
        assert_eq!(
            web_url("git@github.com:org/repo.git").as_deref(),
            Some("https://github.com/org/repo")
        );

        assert_eq!(
            web_url("https://github.com/org/repo.git").as_deref(),
            Some("https://github.com/org/repo")
        );

        assert_eq!(
            web_url("ssh://git@gitlab.com/org/repo.git").as_deref(),
            Some("https://gitlab.com/org/repo")
        );

        assert_eq!(web_url("file:///srv/docs"), None);
        assert_eq!(web_url("/srv/docs"), None);
    }
}
