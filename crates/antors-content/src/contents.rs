//! Where a collected file's bytes come from.
//!
//! A build reads files from two places, and the difference matters to more than
//! the reading. A file in the worktree is *on disk*: it can be read when it is
//! wanted rather than up front, and it has a path a watching build can watch.
//! A file from another ref exists only as an object, which is read once while
//! the ref is open and then carried.

use std::{
    io,
    path::{
        Path,
        PathBuf,
    },
    sync::Arc,
};

/// A collected file's bytes, or the way to get them.
#[derive(Clone, Debug)]
pub enum Contents {
    /// A file in a worktree, read when it is wanted.
    ///
    /// Reading late is what lets `antors serve` re-render after an edit without
    /// collecting the whole site again, and the path is what it watches.
    Path(PathBuf),

    /// Bytes from a git object, read while the ref was open.
    ///
    /// Shared rather than copied: the same file may be reached by several
    /// resource IDs, and an image is not small.
    Bytes(Arc<[u8]>),
}

impl Contents {
    /// The bytes.
    pub fn read(&self) -> io::Result<Vec<u8>> {
        match self {
            Self::Path(path) => std::fs::read(path),
            Self::Bytes(bytes) => Ok(bytes.to_vec()),
        }
    }

    /// The bytes as text.
    pub fn read_to_string(&self) -> io::Result<String> {
        match self {
            Self::Path(path) => std::fs::read_to_string(path),

            Self::Bytes(bytes) => String::from_utf8(bytes.to_vec())
                .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error)),
        }
    }

    /// The file on disk, for a build that wants to watch it.
    pub fn path(&self) -> Option<&Path> {
        match self {
            Self::Path(path) => Some(path),
            Self::Bytes(_) => None,
        }
    }

    /// How to name this file in a message.
    ///
    /// A path names itself; bytes out of an object have no name of their own,
    /// so the caller's resource ID is the only name there is.
    pub fn describe(&self, resource: &str) -> String {
        match self {
            Self::Path(path) => path.display().to_string(),
            Self::Bytes(_) => resource.to_string(),
        }
    }
}

impl From<PathBuf> for Contents {
    fn from(path: PathBuf) -> Self {
        Self::Path(path)
    }
}

impl From<Vec<u8>> for Contents {
    fn from(bytes: Vec<u8>) -> Self {
        Self::Bytes(bytes.into())
    }
}

#[cfg(test)]
mod tests {
    #![expect(clippy::unwrap_used, reason = "a failed read is the test failing")]

    use super::*;

    #[test]
    fn bytes_read_back() {
        let contents = Contents::from(b"= A Page\n".to_vec());

        assert_eq!(contents.read().unwrap(), b"= A Page\n");
        assert_eq!(contents.read_to_string().unwrap(), "= A Page\n");
        assert_eq!(contents.path(), None);
    }

    #[test]
    fn bytes_that_are_not_text_say_so() {
        let contents = Contents::from(vec![0xff, 0xfe]);

        assert!(contents.read_to_string().is_err());
        assert!(contents.read().is_ok());
    }

    #[test]
    fn a_path_is_what_a_watching_build_watches() {
        let contents = Contents::from(PathBuf::from("/docs/a.adoc"));

        assert_eq!(contents.path(), Some(Path::new("/docs/a.adoc")));
    }
}
