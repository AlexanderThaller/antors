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
        // a partial beside a partial, an example beside an example.
        let mut resolved = id.resolve_in(&context, from.family);

        if id.is_bare() {
            // …and a bare target may still climb out of its family, which is
            // how a page reaches `../examples/config.yaml`. The join is done on
            // the *module*-relative path, so the family is whatever directory
            // the climb lands in rather than the one it started from.
            let (family, relative) = beside(from.family, &from.relative, target)?;

            resolved.family = family;
            resolved.relative = relative;
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

/// Where a bare relative target lands, starting from a file in `family` at
/// `relative`.
///
/// The arithmetic is done on the path from the *module* root — `pages/a.adoc`
/// rather than `a.adoc` — because that is the only frame in which `..` means
/// what the author meant. A target that stays put keeps its family; one that
/// climbs into a sibling directory takes that directory's family, which is why
/// `include::../examples/config.yaml[]` from a page finds an example.
fn beside(family: Family, relative: &str, target: &str) -> Option<(Family, String)> {
    let directory = family.directory()?;
    let joined = normalize(&format!("{directory}/{relative}/../{target}"));

    let (head, rest) = joined.split_once('/')?;

    // A climb that lands somewhere that is not a family is a target pointing
    // outside the module, which no resource ID can name.
    let family = Family::from_directory(head)?;

    (!rest.is_empty()).then(|| (family, rest.to_string()))
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

        match file.contents.read() {
            Ok(bytes) => match String::from_utf8(bytes) {
                Ok(content) => {
                    // Only a file on disk can be watched; a blob read out of a
                    // ref cannot change without the ref changing.
                    if let Some(path) = file.contents.path() {
                        self.read.borrow_mut().push(path.to_path_buf());
                    }

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
    fn a_bare_target_stays_in_the_same_directory() {
        assert_eq!(
            beside(Family::Partial, "sub/one.adoc", "two.adoc"),
            Some((Family::Partial, "sub/two.adoc".to_string()))
        );

        assert_eq!(
            beside(Family::Partial, "one.adoc", "two.adoc"),
            Some((Family::Partial, "two.adoc".to_string()))
        );
    }

    #[test]
    fn a_bare_target_may_climb_within_its_family() {
        assert_eq!(
            beside(Family::Partial, "sub/deep/one.adoc", "../two.adoc"),
            Some((Family::Partial, "sub/two.adoc".to_string()))
        );
    }

    #[test]
    fn a_bare_target_may_climb_into_another_family() {
        // The form a page uses to reach an example without naming its family.
        assert_eq!(
            beside(Family::Page, "0002-design.adoc", "../examples/crd.yaml"),
            Some((Family::Example, "crd.yaml".to_string()))
        );

        assert_eq!(
            beside(Family::Page, "sub/a.adoc", "../../partials/note.adoc"),
            Some((Family::Partial, "note.adoc".to_string()))
        );
    }

    #[test]
    fn climbing_out_of_the_module_names_nothing() {
        assert_eq!(beside(Family::Page, "one.adoc", "../../../x.adoc"), None);
        assert_eq!(
            beside(Family::Page, "one.adoc", "../elsewhere/x.adoc"),
            None
        );
    }

    #[test]
    fn normalize_removes_self_references() {
        assert_eq!(normalize("a/./b/../c.adoc"), "a/c.adoc");
    }
}
