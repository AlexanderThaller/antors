//! Reading content sources off the file system.
//!
//! This is the half of the build that touches directories, and it is kept as
//! thin as it can be: walk each start path, decide which family each file
//! belongs to from where it sits, and hand the result to the [`Catalog`]. Every
//! decision that is about the *site* rather than about the disk — which version
//! is latest, what a URL looks like, whether a reference resolves — is made
//! there instead.
//!
//! Only worktrees are read. A content source that names a remote repository is
//! reported rather than fetched, because a half-supported `url:` that quietly
//! collected the wrong revision would be worse than one that says it cannot.

use std::{
    fs,
    path::{
        Path,
        PathBuf,
    },
    sync::Arc,
};

use antors_model::{
    descriptor::Descriptor,
    playbook::{
        Playbook,
        Source,
        is_local,
    },
    resource::{
        Family,
        Key,
    },
};

use crate::{
    catalog::{
        Catalog,
        CatalogError,
        ComponentVersion,
    },
    contents::Contents,
    git,
    origin::{
        Origin,
        RefType,
        web_url,
    },
};

/// Why content could not be collected.
#[derive(Debug, thiserror::Error)]
pub enum AggregateError {
    /// A content source named a repository that is not on this machine.
    #[error(
        "`{url}` is a remote repository, which this build cannot fetch yet: clone it and point \
         the content source at the clone"
    )]
    RemoteSource {
        /// The URL the playbook gave.
        url: String,
    },

    /// A start path had no `antora.yml`, so it describes no component version.
    #[error("`{}` has no antora.yml", path.display())]
    NoDescriptor {
        /// The directory that was searched.
        path: PathBuf,
    },

    /// A directory could not be read.
    #[error("reading `{}`", path.display())]
    Read {
        /// The path that could not be read.
        path: PathBuf,

        /// What the file system said.
        #[source]
        error: std::io::Error,
    },

    /// Nothing said what version a component version is.
    #[error("in `{}`", path.display())]
    Version {
        /// The descriptor that said nothing.
        path: PathBuf,

        /// What was missing.
        #[source]
        error: antors_model::descriptor::MissingVersion,
    },

    /// An `antora.yml` was not a component descriptor.
    #[error("parsing `{}`", path.display())]
    Descriptor {
        /// The descriptor that could not be parsed.
        path: PathBuf,

        /// What the YAML parser said.
        #[source]
        error: Box<serde_yaml_ng::Error>,
    },

    /// Two files claimed one resource ID.
    ///
    /// Boxed because a catalog error carries two whole paths and a resource ID,
    /// and every `Result` in this module would otherwise be that wide on the
    /// success path too.
    #[error(transparent)]
    Catalog(#[from] Box<CatalogError>),
}

/// What a build should be told about, that did not stop it.
///
/// A playbook may ask for things this build cannot do — a branch it cannot
/// read, a UI bundle it cannot unpack, an extension it cannot run. None of
/// those is a reason to produce nothing, and all of them are reasons the site
/// will not be what the author expected, so each is carried out with the
/// catalog rather than swallowed.
#[derive(Clone, Debug, thiserror::Error)]
pub enum Notice {
    /// A `branches:` or `tags:` pattern matched no ref in the repository.
    #[error("`{url}` asks for `{pattern}`, which matches no {kind} there")]
    NoSuchRefs {
        /// The source that asked.
        url: String,

        /// What it asked for, as written.
        pattern: String,

        /// Whether it was asking for branches or for tags.
        kind: &'static str,
    },

    /// A ref matched but could not be read.
    #[error("`{url}` at `{refname}`: {reason}")]
    UnreadableRef {
        /// The source the ref belongs to.
        url: String,

        /// The ref.
        refname: String,

        /// Why it could not be read.
        reason: String,
    },
}

/// A catalog, and what the build should be told about collecting it.
#[derive(Debug)]
pub struct Aggregated {
    /// Every resource that was collected.
    pub catalog: Catalog,

    /// What the playbook asked for that this build could not do.
    pub notices: Vec<Notice>,
}

/// Collect every content source the playbook names.
pub fn aggregate(playbook: &Playbook) -> Result<Aggregated, AggregateError> {
    let mut aggregated = Aggregated {
        catalog: Catalog::new(playbook.urls.html_extension_style),
        notices: Vec::new(),
    };

    for source in &playbook.content.sources {
        collect_source(playbook, source, &mut aggregated)?;
    }

    Ok(aggregated)
}

/// Collect every ref and start path of one source.
fn collect_source(
    playbook: &Playbook,
    source: &Source,
    aggregated: &mut Aggregated,
) -> Result<(), AggregateError> {
    if !is_local(&source.url) {
        return Err(AggregateError::RemoteSource {
            url: source.url.clone(),
        });
    }

    let root = PathBuf::from(&source.url);
    let repository = git::Repository::discover(&root).ok();

    let edit_url_pattern = source
        .edit_url
        .clone()
        .or_else(|| playbook.content.edit_url.clone());

    let worktrees = source.worktrees(&playbook.content);
    let version = source.version(&playbook.content);

    for reference in wanted_refs(source, playbook, repository.as_ref(), aggregated) {
        // The checked-out ref is read from the worktree: it is already on disk,
        // already filtered — Git LFS among them — and it is the one an author
        // is editing, which is what a watching build needs to watch.
        let from_worktree = reference.is_head
            && worktrees.includes(&reference.name, Some(&reference.name))
            && repository
                .as_ref()
                .is_none_or(|repository| repository.workdir().is_some());

        for start_path in source.start_paths() {
            let origin = Arc::new(Origin {
                url: Some(source.url.clone()),
                web_url: repository
                    .as_ref()
                    .and_then(git::Repository::remote_url)
                    .as_deref()
                    .and_then(web_url),
                refname: reference.name.clone(),
                reftype: reference.kind,
                start_path: start_path.clone(),
                worktree: from_worktree
                    .then(|| {
                        repository
                            .as_ref()
                            .and_then(|repository| repository.workdir().map(Path::to_path_buf))
                    })
                    .flatten()
                    .or_else(|| from_worktree.then(|| root.clone())),
                edit_url_pattern: edit_url_pattern.clone(),
            });

            let files = if from_worktree {
                match worktree_files(&root.join(&start_path)) {
                    Ok(files) => files,

                    Err(error) => {
                        return Err(error);
                    }
                }
            } else {
                let Some(repository) = repository.as_ref() else {
                    continue;
                };

                match repository.read_tree(&reference.name, &start_path) {
                    Ok(entries) => entries
                        .into_iter()
                        .map(|(entry, bytes)| (entry.path, Contents::from(bytes)))
                        .collect(),

                    Err(error) => {
                        aggregated.notices.push(Notice::UnreadableRef {
                            url: source.url.clone(),
                            refname: reference.name.clone(),
                            reason: error.to_string(),
                        });

                        continue;
                    }
                }
            };

            collect_component_version(
                &files,
                &origin,
                version,
                &root.join(&start_path),
                &mut aggregated.catalog,
            )?;
        }
    }

    Ok(())
}

/// One ref a source asked for and the repository has.
#[derive(Clone, Debug)]
struct Ref {
    /// Its short name.
    name: String,

    /// Whether it is a branch or a tag.
    kind: RefType,

    /// Whether it is the one checked out.
    is_head: bool,
}

/// Which refs a source asks for, of those the repository has.
///
/// A pattern that matches nothing is reported rather than ignored: a playbook
/// naming a branch that was renamed would otherwise publish a site quietly
/// missing a version.
fn wanted_refs(
    source: &Source,
    playbook: &Playbook,
    repository: Option<&git::Repository>,
    aggregated: &mut Aggregated,
) -> Vec<Ref> {
    let head = repository.and_then(git::Repository::head);

    // A directory that is not in a repository is still a directory of content.
    // It has one ref, which is whatever is there.
    let Some(repository) = repository else {
        return vec![Ref {
            name: head.unwrap_or_else(|| "HEAD".to_string()),
            kind: RefType::Branch,
            is_head: true,
        }];
    };

    let branches = repository.branches();
    let tags = repository.tags();

    let mut wanted: Vec<Ref> = Vec::new();

    for (patterns, kind, available) in [
        (
            source.branches(&playbook.content),
            RefType::Branch,
            &branches,
        ),
        (source.tags(&playbook.content), RefType::Tag, &tags),
    ] {
        for pattern in patterns {
            // `HEAD` is how a playbook says "whatever is checked out", and is
            // not a ref name to match against.
            let matched: Vec<&String> = if pattern == "HEAD" || pattern == "." {
                head.iter().collect()
            } else {
                available
                    .iter()
                    .filter(|name| matches_pattern(&pattern, name))
                    .collect()
            };

            if matched.is_empty() {
                aggregated.notices.push(Notice::NoSuchRefs {
                    url: source.url.clone(),
                    pattern: pattern.clone(),
                    kind: if kind == RefType::Branch {
                        "branch"
                    } else {
                        "tag"
                    },
                });

                continue;
            }

            for name in matched {
                if wanted.iter().any(|reference| reference.name == *name) {
                    continue;
                }

                wanted.push(Ref {
                    name: name.clone(),
                    kind,
                    is_head: head.as_deref() == Some(name.as_str()),
                });
            }
        }
    }

    wanted
}

/// Whether a `branches:` or `tags:` glob matches a ref name.
///
/// The shapes a real playbook uses are a literal name, a trailing `*`, and a
/// `{0..9}` digit class — `v{0..9}*` is Antora's own default for tags.
fn matches_pattern(pattern: &str, name: &str) -> bool {
    let mut rest = name;
    let mut chars = pattern.chars().peekable();

    while let Some(character) = chars.next() {
        match character {
            '*' => {
                if chars.peek().is_none() {
                    return true;
                }

                // A `*` in the middle is more than this understands, and
                // guessing would publish the wrong branch.
                return false;
            }

            '{' => {
                let class: String = chars.by_ref().take_while(|c| *c != '}').collect();

                let Some((low, high)) = class.split_once("..") else {
                    return false;
                };

                let (Some(low), Some(high), Some(next)) =
                    (low.chars().next(), high.chars().next(), rest.chars().next())
                else {
                    return false;
                };

                if next < low || next > high {
                    return false;
                }

                rest = &rest[next.len_utf8()..];
            }

            literal => match rest.strip_prefix(literal) {
                Some(tail) => rest = tail,
                None => return false,
            },
        }
    }

    rest.is_empty()
}

/// Every file under a worktree directory, as paths relative to it.
fn worktree_files(root: &Path) -> Result<Vec<(String, Contents)>, AggregateError> {
    if !root.is_dir() {
        return Ok(Vec::new());
    }

    Ok(walk(root)?
        .into_iter()
        .filter_map(|path| {
            let relative = relative_to(root, &path)?;
            Some((relative, Contents::from(path)))
        })
        .collect())
}

/// Collect one start path: its descriptor, its modules and its navigation.
///
/// `files` is every file under the start path, however it was read — from a
/// worktree or from a ref's tree. Which of the two it was is not a question
/// this has to ask: a component version is the same component version either
/// way, and keeping the difference to one place above is what makes that true.
fn collect_component_version(
    files: &[(String, Contents)],
    origin: &Arc<Origin>,
    version: Option<&antors_model::playbook::VersionSpec>,
    describe: &Path,
    catalog: &mut Catalog,
) -> Result<(), AggregateError> {
    let Some((_, descriptor_contents)) = files.iter().find(|(path, _)| path == DESCRIPTOR_FILENAME)
    else {
        return Err(AggregateError::NoDescriptor {
            path: describe.to_path_buf(),
        });
    };

    let descriptor_path = describe.join(DESCRIPTOR_FILENAME);

    let source = descriptor_contents
        .read_to_string()
        .map_err(|error| AggregateError::Read {
            path: descriptor_path.clone(),
            error,
        })?;

    let mut descriptor: Descriptor =
        serde_yaml_ng::from_str(&source).map_err(|error| AggregateError::Descriptor {
            path: descriptor_path.clone(),
            error: Box::new(error),
        })?;

    // The descriptor may name its own version, or leave it to the content
    // source — which is how one `antora.yml`, committed to several branches,
    // describes several versions without being edited on each.
    descriptor
        .resolve_version(version, &origin.refname)
        .map_err(|error| AggregateError::Version {
            path: descriptor_path.clone(),
            error,
        })?;

    let component = descriptor.name.clone();
    let version = descriptor.version();

    for (path, contents) in files {
        let Some(key) = classify(path, &component, &version) else {
            continue;
        };

        catalog.add(key, Arc::clone(origin), contents.clone(), path.clone())?;
    }

    // Navigation files are named by path rather than found by convention, so
    // they are collected from the descriptor rather than from the walk — and
    // the module each belongs to is read back out of its path.
    let mut nav = Vec::new();

    for entry in descriptor.nav.clone() {
        let Some((_, contents)) = files.iter().find(|(path, _)| *path == entry) else {
            continue;
        };

        let key = Key {
            component: component.clone(),
            version: version.clone(),
            module: module_of(&entry)
                .unwrap_or_else(|| antors_model::resource::ROOT_MODULE.to_string()),
            family: Family::Nav,
            relative: entry.clone(),
        };

        catalog.add(
            key.clone(),
            Arc::clone(origin),
            contents.clone(),
            entry.clone(),
        )?;

        nav.push(key);
    }

    catalog.add_component_version(ComponentVersion {
        descriptor,
        origin: Arc::clone(origin),
        nav,
    });

    Ok(())
}

/// The file that turns a directory into a component version.
const DESCRIPTOR_FILENAME: &str = "antora.yml";

/// What resource a file within a start path is, if it is one.
///
/// The whole of the classification is in the path: `modules/<module>/<family
/// directory>/<the rest>`. Anything that does not have that shape — a README
/// beside the descriptor, a `.gitignore`, a directory nobody named — is not a
/// resource and is left alone.
fn classify(path: &str, component: &str, version: &str) -> Option<Key> {
    let mut segments = path.split('/');

    if segments.next()? != "modules" {
        return None;
    }

    let module = segments.next()?;
    let family = Family::from_directory(segments.next()?)?;
    let relative: String = segments.collect::<Vec<&str>>().join("/");

    if relative.is_empty() || is_hidden(&relative, family) {
        return None;
    }

    // Only `AsciiDoc` becomes a page. A stray file under `pages/` is not a page
    // with an unusual extension, it is something that was put in the wrong
    // place, and publishing it at a page's URL would be the wrong kind of
    // helpful.
    //
    // The comparison is case sensitive on purpose: a resource ID names a file
    // exactly, so a `README.ADOC` that became a page here would be a page
    // nothing could link to.
    #[expect(
        clippy::case_sensitive_file_extension_comparisons,
        reason = "a resource ID names its file exactly; see above"
    )]
    if family == Family::Page && !relative.ends_with(".adoc") {
        return None;
    }

    Some(Key {
        component: component.to_string(),
        version: version.to_string(),
        module: module.to_string(),
        family,
        relative,
    })
}

/// The module a descriptor-relative path belongs to.
///
/// A navigation file is listed as `modules/<name>/nav.adoc`, and the module is
/// the segment after `modules`. Anything else in the list is left to the
/// caller's default.
fn module_of(entry: &str) -> Option<String> {
    let mut segments = entry.split('/');

    (segments.next()? == "modules").then(|| segments.next().map(str::to_string))?
}

/// Whether a file is kept out of its family.
///
/// A leading underscore marks a file — or a directory — as scaffolding rather
/// than content. It matters only for pages: a `_attributes.adoc` beside them is
/// there to be included, and publishing it would put a fragment at a URL. In
/// the include-only families every file is scaffolding already, so the
/// convention would only get in the way.
fn is_hidden(relative: &str, family: Family) -> bool {
    if family != Family::Page {
        return false;
    }

    relative.split('/').any(|segment| segment.starts_with('_'))
}

/// Every file under `root`, recursively, in sorted order.
fn walk(root: &Path) -> Result<Vec<PathBuf>, AggregateError> {
    let mut files = Vec::new();
    let mut pending = vec![root.to_path_buf()];

    while let Some(directory) = pending.pop() {
        for entry in read_dir(&directory)? {
            let name = entry.file_name();
            let name = name.to_string_lossy();

            // A dot file is the file system's business, not the site's.
            if name.starts_with('.') {
                continue;
            }

            let path = entry.path();

            if path.is_dir() {
                pending.push(path);
            } else if path.is_file() {
                files.push(path);
            }
        }
    }

    files.sort();

    Ok(files)
}

/// Read a directory, naming it in the error if that fails.
fn read_dir(path: &Path) -> Result<impl Iterator<Item = fs::DirEntry>, AggregateError> {
    let entries = fs::read_dir(path).map_err(|error| AggregateError::Read {
        path: path.to_path_buf(),
        error,
    })?;

    Ok(entries.filter_map(Result::ok))
}

/// `path` relative to `root`, with `/` separators.
fn relative_to(root: &Path, path: &Path) -> Option<String> {
    let relative = path.strip_prefix(root).ok()?;

    let joined = relative
        .components()
        .map(|component| component.as_os_str().to_string_lossy())
        .collect::<Vec<_>>()
        .join("/");

    (!joined.is_empty()).then_some(joined)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_nav_path_names_its_module() {
        assert_eq!(
            module_of("modules/guide/nav.adoc").as_deref(),
            Some("guide")
        );

        assert_eq!(module_of("modules/ROOT/nav.adoc").as_deref(), Some("ROOT"));
        assert_eq!(module_of("nav.adoc"), None);
    }

    #[test]
    fn underscores_hide_pages_and_nothing_else() {
        assert!(is_hidden("_attributes.adoc", Family::Page));
        assert!(is_hidden("_shared/note.adoc", Family::Page));
        assert!(!is_hidden("page.adoc", Family::Page));

        // An example named after a private field is still an example.
        assert!(!is_hidden("_private.rs", Family::Example));
    }
}
