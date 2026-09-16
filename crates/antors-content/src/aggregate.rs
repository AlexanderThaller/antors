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
    /// A source asked for refs that are not the checked-out worktree.
    #[error(
        "`{url}` asks for {refs}, and this build reads only the checked-out worktree (`{current}`)"
    )]
    UnreadableRefs {
        /// The source that asked.
        url: String,

        /// What it asked for, as written.
        refs: String,

        /// What is actually checked out.
        current: String,
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

/// Collect every start path of one source.
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

    let worktree = PathBuf::from(&source.url);
    let repository = Repository::discover(&worktree);

    // Only the checked-out worktree is read, so a source that asks for other
    // refs gets what it asked for in part. Saying which part is the difference
    // between a site that is missing a version and a site that is missing a
    // version *and* nobody knows.
    let unreadable: Vec<String> = source
        .branches(&playbook.content)
        .into_iter()
        .chain(source.tags(&playbook.content))
        .filter(|pattern| !names_the_worktree(pattern, &repository.refname))
        .collect();

    if !unreadable.is_empty() {
        aggregated.notices.push(Notice::UnreadableRefs {
            url: source.url.clone(),
            refs: unreadable.join("`, `"),
            current: repository.refname.clone(),
        });
    }

    let edit_url_pattern = source
        .edit_url
        .clone()
        .or_else(|| playbook.content.edit_url.clone());

    for start_path in source.start_paths() {
        let root = worktree.join(&start_path);

        let origin = Arc::new(Origin {
            url: Some(source.url.clone()),
            web_url: repository.remote.as_deref().and_then(web_url),
            refname: repository.refname.clone(),

            // Only a worktree is read, and a worktree is always at a branch —
            // a detached HEAD included, which git itself calls a branch-shaped
            // thing rather than a tag.
            reftype: RefType::Branch,

            start_path: start_path.clone(),
            worktree: Some(repository.root.clone()),
            edit_url_pattern: edit_url_pattern.clone(),
        });

        collect_component_version(
            &root,
            &origin,
            source.version(&playbook.content),
            &mut aggregated.catalog,
        )?;
    }

    Ok(())
}

/// Whether a `branches:` or `tags:` pattern names the ref that is checked out.
///
/// `HEAD` and `.` are how a playbook says "whatever is checked out". Everything
/// else is matched by name, or by a glob — the shapes a real playbook uses are
/// a literal name and a trailing `*`, and anything more elaborate is treated as
/// not matching, which reports the ref rather than quietly reading the wrong
/// one.
fn names_the_worktree(pattern: &str, current: &str) -> bool {
    if pattern == "HEAD" || pattern == "." || pattern == "*" {
        return true;
    }

    match pattern.split_once('*') {
        None => pattern == current,

        // A `*` in the middle, or more than one, is more than this
        // understands.
        Some((prefix, "")) => current.starts_with(prefix),
        Some(_) => false,
    }
}

/// Collect one start path: its descriptor, its modules and its navigation.
fn collect_component_version(
    root: &Path,
    origin: &Arc<Origin>,
    version: Option<&antors_model::playbook::VersionSpec>,
    catalog: &mut Catalog,
) -> Result<(), AggregateError> {
    let descriptor_path = root.join("antora.yml");

    if !descriptor_path.is_file() {
        return Err(AggregateError::NoDescriptor {
            path: root.to_path_buf(),
        });
    }

    let source = fs::read_to_string(&descriptor_path).map_err(|error| AggregateError::Read {
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

    let modules = root.join("modules");

    for module in directories(&modules)? {
        let name = module
            .file_name()
            .map(|name| name.to_string_lossy().into_owned())
            .unwrap_or_default();

        collect_module(&module, &name, &component, &version, origin, root, catalog)?;
    }

    // Navigation files are named by path rather than found by convention, so
    // they are collected from the descriptor rather than from the walk — and
    // the module each belongs to is read back out of its path.
    let mut nav = Vec::new();

    for entry in descriptor.nav.clone() {
        let path = root.join(&entry);

        if !path.is_file() {
            continue;
        }

        let key = Key {
            component: component.clone(),
            version: version.clone(),
            module: module_of(&entry)
                .unwrap_or_else(|| antors_model::resource::ROOT_MODULE.to_string()),
            family: Family::Nav,
            relative: entry.clone(),
        };

        catalog.add(key.clone(), Arc::clone(origin), path, entry.clone())?;
        nav.push(key);
    }

    catalog.add_component_version(ComponentVersion {
        descriptor,
        origin: Arc::clone(origin),
        nav,
    });

    Ok(())
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

/// Collect one module's families.
fn collect_module(
    module_root: &Path,
    module: &str,
    component: &str,
    version: &str,
    origin: &Arc<Origin>,
    component_root: &Path,
    catalog: &mut Catalog,
) -> Result<(), AggregateError> {
    for family in [
        Family::Page,
        Family::Partial,
        Family::Example,
        Family::Image,
        Family::Attachment,
    ] {
        let Some(directory) = family.directory() else {
            continue;
        };

        let root = module_root.join(directory);

        if !root.is_dir() {
            continue;
        }

        for path in walk(&root)? {
            let Some(relative) = relative_to(&root, &path) else {
                continue;
            };

            if is_hidden(&relative, family) {
                continue;
            }

            // Only `AsciiDoc` becomes a page. A stray file under `pages/` is
            // not a page with an unusual extension, it is something that was
            // put in the wrong place, and publishing it at a page's URL would
            // be the wrong kind of helpful.
            //
            // The comparison is case sensitive on purpose: a resource ID names
            // a file exactly, so a `README.ADOC` that became a page here would
            // be a page nothing could link to.
            #[expect(
                clippy::case_sensitive_file_extension_comparisons,
                reason = "a resource ID names its file exactly; see above"
            )]
            if family == Family::Page && !relative.ends_with(".adoc") {
                continue;
            }

            let key = Key {
                component: component.to_string(),
                version: version.to_string(),
                module: module.to_string(),
                family,
                relative,
            };

            let relative_src_path = relative_to(component_root, &path).unwrap_or_default();

            catalog.add(key, Arc::clone(origin), path, relative_src_path)?;
        }
    }

    Ok(())
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

/// The directories directly under `root`, in sorted order. An absent `root` has
/// none, which is not an error: a component may have no modules yet.
fn directories(root: &Path) -> Result<Vec<PathBuf>, AggregateError> {
    if !root.is_dir() {
        return Ok(Vec::new());
    }

    let mut directories: Vec<PathBuf> = read_dir(root)?
        .map(|entry| entry.path())
        .filter(|path| path.is_dir())
        .filter(|path| {
            path.file_name()
                .is_none_or(|name| !name.to_string_lossy().starts_with('.'))
        })
        .collect();

    directories.sort();

    Ok(directories)
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

/// What a worktree can say about itself without a git library.
///
/// Only two things are needed — the branch a worktree is on, and the remote it
/// came from — and both are plain text in `.git`. Reading them directly keeps a
/// git implementation out of the build until there is a reason for one, which
/// is fetching refs rather than describing the current checkout.
#[derive(Clone, Debug)]
struct Repository {
    /// The worktree root, which is the directory `.git` sits in.
    root: PathBuf,

    /// The checked-out branch, or `HEAD` when it cannot be determined.
    refname: String,

    /// The `origin` remote's URL, if there is one.
    remote: Option<String>,
}

impl Repository {
    /// Find the repository `start` is inside, falling back to `start` itself.
    fn discover(start: &Path) -> Self {
        let absolute = start.canonicalize().unwrap_or_else(|_| start.to_path_buf());

        let root = absolute
            .ancestors()
            .find(|directory| directory.join(".git").exists())
            .unwrap_or(&absolute)
            .to_path_buf();

        let git = root.join(".git");

        Self {
            refname: head(&git).unwrap_or_else(|| "HEAD".to_string()),
            remote: remote(&git),
            root,
        }
    }
}

/// The branch name in `.git/HEAD`, or `None` for a detached HEAD.
fn head(git: &Path) -> Option<String> {
    let head = fs::read_to_string(git.join("HEAD")).ok()?;

    Some(head.trim().strip_prefix("ref: refs/heads/")?.to_string())
}

/// The `origin` remote's URL from `.git/config`.
///
/// This reads the INI file rather than shelling out to git, which keeps the
/// build from depending on a git binary being installed for something this
/// small.
fn remote(git: &Path) -> Option<String> {
    let config = fs::read_to_string(git.join("config")).ok()?;
    let mut in_origin = false;

    for line in config.lines() {
        let line = line.trim();

        if line.starts_with('[') {
            in_origin = line.starts_with("[remote \"origin\"]");
            continue;
        }

        if !in_origin {
            continue;
        }

        if let Some((key, value)) = line.split_once('=')
            && key.trim() == "url"
        {
            return Some(value.trim().to_string());
        }
    }

    None
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
