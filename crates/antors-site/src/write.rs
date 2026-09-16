//! Putting files where they go.

use std::path::{
    Path,
    PathBuf,
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
}

impl Writer {
    /// A writer for one output directory.
    pub(crate) fn new(root: PathBuf) -> Self {
        Self {
            root,
            pages: 0,
            files: 0,
        }
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

    /// Copy a file in from a content source.
    pub(crate) fn copy(&mut self, path: &str, from: &Path) -> Result<(), BuildError> {
        let destination = self.root.join(path);
        Self::parent(&destination)?;

        std::fs::copy(from, &destination).map_err(|error| BuildError::Write {
            path: destination,
            error,
        })?;

        self.files += 1;

        Ok(())
    }

    /// Write bytes, creating the directories they go in.
    fn write(&self, path: &str, contents: &[u8]) -> Result<(), BuildError> {
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
