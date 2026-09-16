//! Resolving `include::` against the catalog rather than against the disk.
//!
//! In Antora an include target is a [resource ID], so
//! `include::partial$x.adoc[]` finds the same file from every page in the
//! component version, however deep the page sits. That is the whole point: a
//! partial has one name, not one name per page that uses it.
//!
//! [resource ID]: antors_model::ResourceId

use std::{
    cell::RefCell,
    collections::HashMap,
    path::PathBuf,
    sync::Arc,
};

use antors_content::Catalog;
use antors_model::{
    Family,
    ResourceId,
    resource::{
        Context,
        Key,
    },
};
use asciidoc_parser::{
    Parser,
    attributes::Attrlist,
    parser::{
        IncludeContent,
        IncludeFileHandler,
        IncludeResolution,
    },
};

/// Reads `include::` targets out of the catalog.
///
/// # Resolving an include inside an include
///
/// A partial that includes `sibling.adoc` means the partial beside it, not a
/// partial at the family root — so resolving a nested directive needs to know
/// which file the directive was written in. The parser reports that as the
/// *target string the including file was named by*, which is the string this
/// handler was previously asked to resolve. So each resolution is remembered
/// under that string, and a nested directive looks its parent up to find the
/// context to resolve against.
#[derive(Debug)]
pub struct Resolver {
    /// Every resource in the site.
    ///
    /// Owned rather than borrowed because the parser keeps its include handler
    /// behind an `Rc` for the life of the parse and so requires a `'static`
    /// one; the catalog is shared, not copied.
    catalog: Arc<Catalog>,

    /// The page being parsed, which is what a top-level directive resolves
    /// against.
    page: Key,

    /// What each target string resolved to, so a nested directive can find its
    /// parent's context.
    resolved: RefCell<HashMap<String, Key>>,

    /// Every file read, for a watching build to watch.
    read: RefCell<Vec<PathBuf>>,

    /// Targets that could not be resolved, so the build can report them
    /// against the page that wrote them.
    missing: RefCell<Vec<String>>,
}

impl Resolver {
    /// A resolver for one page.
    pub fn new(catalog: Arc<Catalog>, page: Key) -> Self {
        Self {
            catalog,
            page,
            resolved: RefCell::new(HashMap::new()),
            read: RefCell::new(Vec::new()),
            missing: RefCell::new(Vec::new()),
        }
    }

    /// Every file the parse read, in the order it read them.
    pub fn read(&self) -> Vec<PathBuf> {
        self.read.borrow().clone()
    }

    /// Every target that resolved to nothing.
    pub fn missing(&self) -> Vec<String> {
        self.missing.borrow().clone()
    }

    /// The resource a directive written in `source` and naming `target` means.
    fn locate(&self, source: Option<&str>, target: &str) -> Option<Key> {
        let from = self.including_file(source);
        let id = ResourceId::parse(target).ok()?;

        let context = Context {
            component: from.component.clone(),
            version: from.version.clone(),
            module: from.module.clone(),
        };

        // A target that names no family is relative to the file that wrote it:
        // a partial beside a partial, an example beside an example. Only a
        // target that names one may leave the including file's family.
        let default_family = from.family;

        let mut resolved = id.resolve_in(&context, default_family);

        if id.is_bare() {
            resolved.relative = sibling(&from.relative, target);
        }

        self.catalog.resolve(&resolved).map(|file| file.key.clone())
    }

    /// Which file a directive was written in.
    ///
    /// `None` is the page itself. A name this handler has seen before is the
    /// file it resolved that name to. A name it has not is a file it never
    /// supplied, so the page is the best context available.
    fn including_file(&self, source: Option<&str>) -> Key {
        source
            .and_then(|source| self.resolved.borrow().get(source).cloned())
            .unwrap_or_else(|| self.page.clone())
    }
}

/// A path beside `from`, for a target written as a bare relative path.
fn sibling(from: &str, target: &str) -> String {
    let directory = match from.rfind('/') {
        Some(index) => &from[..index],
        None => "",
    };

    let joined = if directory.is_empty() {
        target.to_string()
    } else {
        format!("{directory}/{target}")
    };

    normalize(&joined)
}

/// Collapse `.` and `..` in a `/`-separated path.
fn normalize(path: &str) -> String {
    let mut segments: Vec<&str> = Vec::new();

    for segment in path.split('/') {
        match segment {
            "" | "." => {}

            ".." => {
                // A `..` that would climb above the family root has nowhere to
                // go, and keeping it makes the failure name the path the
                // author wrote rather than a shorter one they did not.
                if matches!(segments.last(), Some(&last) if last != "..") {
                    segments.pop();
                } else {
                    segments.push("..");
                }
            }

            other => segments.push(other),
        }
    }

    segments.join("/")
}

impl IncludeFileHandler for Resolver {
    fn resolve_target(
        &self,
        source: Option<&str>,
        target: &str,
        _attrlist: &Attrlist<'_>,
        _parser: &Parser,
    ) -> IncludeResolution {
        // A URL is not a resource, and fetching one during a build would make
        // the output depend on the network.
        if target.contains("://") {
            self.missing.borrow_mut().push(target.to_string());
            return IncludeResolution::NotFound;
        }

        let Some(key) = self.locate(source, target) else {
            self.missing.borrow_mut().push(target.to_string());
            return IncludeResolution::NotFound;
        };

        let Some(file) = self.catalog.get(&key) else {
            self.missing.borrow_mut().push(target.to_string());
            return IncludeResolution::NotFound;
        };

        match std::fs::read(&file.path) {
            Ok(bytes) => match String::from_utf8(bytes) {
                Ok(content) => {
                    self.read.borrow_mut().push(file.path.clone());

                    // Remembered under the name the parser will hand back when
                    // this file's own directives are resolved.
                    self.resolved.borrow_mut().insert(target.to_string(), key);

                    IncludeContent::new(content).into()
                }

                Err(_) => IncludeResolution::NotDecodable,
            },

            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                IncludeResolution::NotFound
            }

            Err(_) => IncludeResolution::NotReadable,
        }
    }
}

/// The family an `include::` target implies when it names none, for a
/// directive written directly in a page.
///
/// Antora has no default here: a page that includes a bare `notes.adoc` means
/// the page beside it, which is almost never what was meant but is what was
/// written.
pub const PAGE_DEFAULT_FAMILY: Family = Family::Page;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_sibling_stays_in_the_same_directory() {
        assert_eq!(sibling("sub/one.adoc", "two.adoc"), "sub/two.adoc");
        assert_eq!(sibling("one.adoc", "two.adoc"), "two.adoc");
    }

    #[test]
    fn a_sibling_may_climb() {
        assert_eq!(sibling("sub/deep/one.adoc", "../two.adoc"), "sub/two.adoc");
        assert_eq!(sibling("sub/one.adoc", "../two.adoc"), "two.adoc");
    }

    #[test]
    fn climbing_past_the_root_keeps_the_target_as_written() {
        assert_eq!(sibling("one.adoc", "../../two.adoc"), "../../two.adoc");
    }

    #[test]
    fn normalize_removes_self_references() {
        assert_eq!(normalize("a/./b/../c.adoc"), "a/c.adoc");
    }
}
