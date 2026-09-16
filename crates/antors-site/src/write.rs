//! Putting files where they go.

use std::{
    collections::BTreeSet,
    path::{
        Path,
        PathBuf,
    },
};

use crate::build::BuildError;

/// Writes the site into one directory, counting as it goes.
#[derive(Debug)]
pub(crate) struct Writer {
    /// The output directory.
    root: PathBuf,

    /// How many pages have been written.
    pub(crate) pages: usize,

    /// How many other files have been written.
    pub(crate) files: usize,

    /// Every path written, relative to the output directory.
    ///
    /// Kept so the build can tell what was *already* there — see
    /// [`stale`](Self::stale).
    written: BTreeSet<String>,
}

impl Writer {
    /// A writer for one output directory.
    pub(crate) fn new(root: PathBuf) -> Self {
        Self {
            root,
            pages: 0,
            files: 0,
            written: BTreeSet::new(),
        }
    }

    /// The pages in the output directory that this build did not write.
    ///
    /// A site is written over whatever was there before, and a page that has
    /// moved — because a component was versioned, or renamed, or a module
    /// split — leaves its old copy behind at its old URL. That copy still
    /// loads, still looks like a page, and is indistinguishable from a current
    /// one until someone notices it is missing something.
    ///
    /// So the leftovers are counted. Only pages: an output directory may also
    /// hold a `CNAME`, a `.nojekyll` or anything else its owner put there, and
    /// none of those can be mistaken for content.
    pub(crate) fn stale(&self) -> Vec<String> {
        let mut stale = Vec::new();
        let mut pending = vec![self.root.clone()];

        while let Some(directory) = pending.pop() {
            let Ok(entries) = std::fs::read_dir(&directory) else {
                continue;
            };

            for entry in entries.flatten() {
                let path = entry.path();

                if path.is_dir() {
                    pending.push(path);
                    continue;
                }

                if path.extension().is_none_or(|extension| extension != "html") {
                    continue;
                }

                let Ok(relative) = path.strip_prefix(&self.root) else {
                    continue;
                };

                let relative = relative.to_string_lossy().replace('\\', "/");

                if !self.written.contains(&relative) {
                    stale.push(relative);
                }
            }
        }

        stale.sort();

        stale
    }

    /// Empty the output directory.
    ///
    /// Only the directory named in the playbook is removed, and only if it
    /// exists: a build that deleted the wrong directory because its output
    /// path was a typo would be the worst kind of bug to have.
    pub(crate) fn clean(&self) -> Result<(), BuildError> {
        if !self.root.exists() {
            return Ok(());
        }

        std::fs::remove_dir_all(&self.root).map_err(|error| BuildError::Write {
            path: self.root.clone(),
            error,
        })
    }

    /// Write a page.
    pub(crate) fn page(&mut self, path: &str, html: &str) -> Result<(), BuildError> {
        self.write(path, html.as_bytes())?;
        self.pages += 1;

        Ok(())
    }

    /// Write anything else.
    pub(crate) fn file(&mut self, path: &str, contents: &[u8]) -> Result<(), BuildError> {
        self.write(path, contents)?;
        self.files += 1;

        Ok(())
    }

    /// Write bytes, creating the directories they go in.
    fn write(&mut self, path: &str, contents: &[u8]) -> Result<(), BuildError> {
        self.written.insert(path.to_string());

        let destination = self.root.join(path);
        Self::parent(&destination)?;

        std::fs::write(&destination, contents).map_err(|error| BuildError::Write {
            path: destination,
            error,
        })
    }

    /// Make sure a file's directory exists.
    fn parent(path: &Path) -> Result<(), BuildError> {
        let Some(parent) = path.parent() else {
            return Ok(());
        };

        std::fs::create_dir_all(parent).map_err(|error| BuildError::Write {
            path: parent.to_path_buf(),
            error,
        })
    }
}
