//! Reading content out of a repository.
//!
//! A build reads two kinds of ref. The one that is checked out is read from the
//! worktree — it is already on disk, already filtered, and it is the one an
//! author is editing, so a watching build wants the files rather than the
//! objects. Every other ref is read from git's object database.
//!
//! Nothing here writes, fetches or checks anything out. A build reads history;
//! it does not change it.

use std::path::{
    Path,
    PathBuf,
};

/// Why a repository could not be read.
#[derive(Debug, thiserror::Error)]
pub enum GitError {
    /// There is no repository at that path.
    #[error("`{}` is not a git repository", path.display())]
    NotARepository {
        /// Where one was looked for.
        path: PathBuf,

        /// What gix said.
        #[source]
        error: Box<gix::discover::Error>,
    },

    /// A ref the playbook asked for is not in the repository.
    #[error("`{refname}` is not a branch or tag of `{}`", repository.display())]
    NoSuchRef {
        /// The repository.
        repository: PathBuf,

        /// The ref that is not in it.
        refname: String,
    },

    /// A ref exists but does not lead to a tree.
    #[error("`{refname}` does not name a commit")]
    NotACommit {
        /// The ref.
        refname: String,

        /// What gix said.
        #[source]
        error: Box<dyn std::error::Error + Send + Sync>,
    },

    /// The start path is not in that ref's tree.
    #[error("`{start_path}` is not in `{refname}`")]
    NoSuchPath {
        /// The ref that was searched.
        refname: String,

        /// The path that is not in it.
        start_path: String,
    },

    /// An object could not be read.
    #[error("reading `{path}` from `{refname}`")]
    Unreadable {
        /// The ref the object belongs to.
        refname: String,

        /// The file it is.
        path: String,

        /// What gix said.
        #[source]
        error: Box<dyn std::error::Error + Send + Sync>,
    },

    /// A file is stored in Git LFS and its content is not on this machine.
    #[error(
        "`{path}` is stored in Git LFS and has not been fetched: run `git lfs pull` in \
         `{}`",
        repository.display()
    )]
    MissingLfsObject {
        /// The repository whose store was searched.
        repository: PathBuf,

        /// The file whose content is missing.
        path: String,
    },

    /// Something else went wrong reading the repository.
    #[error("reading `{}`", repository.display())]
    Other {
        /// The repository.
        repository: PathBuf,

        /// What gix said.
        #[source]
        error: Box<dyn std::error::Error + Send + Sync>,
    },
}

/// One file in a ref's tree.
#[derive(Clone, Debug)]
pub struct Entry {
    /// Its path within the start path, with `/` separators.
    pub path: String,

    /// The blob it is.
    pub id: gix::ObjectId,
}

/// A repository, open for reading.
#[derive(Clone)]
pub struct Repository {
    /// The repository itself.
    inner: gix::Repository,

    /// Where it is, for naming it in an error.
    root: PathBuf,
}

/// `gix::Repository` has no `Debug`, and a repository's interesting fact is
/// where it is.
impl std::fmt::Debug for Repository {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Repository")
            .field("root", &self.root)
            .finish_non_exhaustive()
    }
}

impl Repository {
    /// Open the repository containing `path`.
    pub fn discover(path: &Path) -> Result<Self, GitError> {
        let inner = gix::discover(path).map_err(|error| GitError::NotARepository {
            path: path.to_path_buf(),
            error: Box::new(error),
        })?;

        let root = inner
            .workdir()
            .unwrap_or_else(|| inner.git_dir())
            .to_path_buf();

        Ok(Self { inner, root })
    }

    /// The worktree root, for a repository that has one.
    pub fn workdir(&self) -> Option<&Path> {
        self.inner.workdir()
    }

    /// The branch that is checked out, if HEAD names one.
    pub fn head(&self) -> Option<String> {
        self.inner
            .head_name()
            .ok()
            .flatten()
            .map(|name| name.shorten().to_string())
    }

    /// The `origin` remote's URL.
    pub fn remote_url(&self) -> Option<String> {
        self.inner
            .find_remote("origin")
            .ok()?
            .url(gix::remote::Direction::Fetch)
            .map(ToString::to_string)
    }

    /// Every local branch, by short name.
    ///
    /// A ref that cannot be read is skipped rather than reported: a repository
    /// with one broken ref should still publish the others, and a ref the
    /// playbook actually asked for is reported by name when it turns out to be
    /// missing.
    pub fn branches(&self) -> Vec<String> {
        let Ok(platform) = self.inner.references() else {
            return Vec::new();
        };

        let Ok(refs) = platform.local_branches() else {
            return Vec::new();
        };

        shortnames(refs)
    }

    /// Every tag, by short name.
    pub fn tags(&self) -> Vec<String> {
        let Ok(platform) = self.inner.references() else {
            return Vec::new();
        };

        let Ok(refs) = platform.tags() else {
            return Vec::new();
        };

        shortnames(refs)
    }

    /// Every file under `start_path` in `refname`, with its contents read.
    ///
    /// Contents are read here rather than left for later because the objects
    /// they come from are reachable only through this repository handle, and
    /// threading one through a catalog that is shared across a whole build
    /// would make the catalog's lifetime the repository's.
    pub fn read_tree(
        &self,
        refname: &str,
        start_path: &str,
    ) -> Result<Vec<(Entry, Vec<u8>)>, GitError> {
        let mut reference =
            self.inner
                .find_reference(refname)
                .map_err(|_| GitError::NoSuchRef {
                    repository: self.root.clone(),
                    refname: refname.to_string(),
                })?;

        let commit = reference
            .peel_to_commit()
            .map_err(|error| GitError::NotACommit {
                refname: refname.to_string(),
                error: Box::new(error),
            })?;

        let tree = commit.tree().map_err(|error| GitError::NotACommit {
            refname: refname.to_string(),
            error: Box::new(error),
        })?;

        let root = if start_path.is_empty() {
            tree
        } else {
            let entry = tree
                .lookup_entry_by_path(start_path)
                .map_err(|error| GitError::Unreadable {
                    refname: refname.to_string(),
                    path: start_path.to_string(),
                    error: Box::new(error),
                })?
                .ok_or_else(|| GitError::NoSuchPath {
                    refname: refname.to_string(),
                    start_path: start_path.to_string(),
                })?;

            entry
                .object()
                .map_err(|error| GitError::Unreadable {
                    refname: refname.to_string(),
                    path: start_path.to_string(),
                    error: Box::new(error),
                })?
                .try_into_tree()
                .map_err(|_| GitError::NoSuchPath {
                    refname: refname.to_string(),
                    start_path: start_path.to_string(),
                })?
        };

        let mut recorder = gix::traverse::tree::Recorder::default();

        root.traverse()
            .breadthfirst(&mut recorder)
            .map_err(|error| GitError::Unreadable {
                refname: refname.to_string(),
                path: start_path.to_string(),
                error: Box::new(error),
            })?;

        let mut files = Vec::new();

        for record in recorder.records {
            // A directory is a step on the way; a symlink or a submodule points
            // somewhere this build does not follow.
            if !record.mode.is_blob() {
                continue;
            }

            let path = record.filepath.to_string();
            let contents = self.read_blob(record.oid, refname, &path)?;

            files.push((
                Entry {
                    path,
                    id: record.oid,
                },
                contents,
            ));
        }

        Ok(files)
    }

    /// Read one blob, following a Git LFS pointer to the object it stands for.
    fn read_blob(&self, id: gix::ObjectId, refname: &str, path: &str) -> Result<Vec<u8>, GitError> {
        let object = self
            .inner
            .find_object(id)
            .map_err(|error| GitError::Unreadable {
                refname: refname.to_string(),
                path: path.to_string(),
                error: Box::new(error),
            })?;

        let data = object.data.clone();

        // A repository read through its objects has not had LFS's smudge filter
        // run over it, so an LFS-tracked file arrives as the pointer rather
        // than as the file. Publishing the pointer would put a 130-byte
        // text file at a `.pdf` URL and say nothing about it.
        let Some(oid) = lfs_pointer(&data) else {
            return Ok(data);
        };

        let stored = self.lfs_object(&oid);

        std::fs::read(&stored).map_err(|_| GitError::MissingLfsObject {
            repository: self.root.clone(),
            path: path.to_string(),
        })
    }

    /// Where an LFS object with this hash is kept.
    fn lfs_object(&self, oid: &str) -> PathBuf {
        self.inner
            .git_dir()
            .join("lfs")
            .join("objects")
            .join(&oid[..2])
            .join(&oid[2..4])
            .join(oid)
    }
}

/// The short names an iterator of refs yields, skipping any that will not read.
fn shortnames<'a>(
    refs: impl Iterator<Item = Result<gix::Reference<'a>, Box<dyn std::error::Error + Send + Sync>>>,
) -> Vec<String> {
    refs.filter_map(|reference| Some(reference.ok()?.name().shorten().to_string()))
        .collect()
}

/// The object hash a Git LFS pointer names, if `data` is one.
///
/// A pointer is a short text file of `key value` lines, the first of which
/// names the spec. Anything that does not start with that line is the file
/// itself.
fn lfs_pointer(data: &[u8]) -> Option<String> {
    const SPEC: &str = "version https://git-lfs.github.com/spec/";

    // A pointer is a few hundred bytes at most, so anything larger is the file.
    // This also keeps a large binary from being scanned as text.
    if data.len() > 1024 {
        return None;
    }

    let text = std::str::from_utf8(data).ok()?;

    if !text.starts_with(SPEC) {
        return None;
    }

    text.lines()
        .find_map(|line| line.strip_prefix("oid sha256:"))
        .filter(|oid| oid.len() >= 4 && oid.chars().all(|c| c.is_ascii_hexdigit()))
        .map(str::to_string)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_lfs_pointer_names_its_object() {
        // Built from its lines rather than written as one literal: `rustfmt` is
        // configured to re-wrap long strings, and a wrapped escape is a
        // different string.
        let pointer = [
            "version https://git-lfs.github.com/spec/v1",
            "oid sha256:4d7a214614ab2935c943f9e0ff69d22eadbb8f32b1258daaa5e2ca24d17e2393",
            "size 12345",
            "",
        ]
        .join("\n");

        assert_eq!(
            lfs_pointer(pointer.as_bytes()).as_deref(),
            Some("4d7a214614ab2935c943f9e0ff69d22eadbb8f32b1258daaa5e2ca24d17e2393")
        );
    }

    #[test]
    fn an_ordinary_file_is_not_a_pointer() {
        assert_eq!(lfs_pointer(b"= A Page\n\nSome text.\n"), None);
        assert_eq!(lfs_pointer(&[0x89, b'P', b'N', b'G']), None);
    }

    #[test]
    fn a_large_file_is_never_scanned_as_one() {
        // A pointer is a few hundred bytes; anything bigger is the file, and
        // reading a megabyte of binary as UTF-8 to find out is wasted work.
        assert_eq!(lfs_pointer(&vec![b'v'; 2048]), None);
    }
}
