//! The playbook: everything about a build that is not in the content.
//!
//! A playbook says where the content comes from, what UI to dress it in, and
//! where the finished site goes. Nothing in it describes a document, and
//! nothing in a document describes a build — which is what lets one set of
//! sources be published as a public site and an internal one without either
//! knowing about the other.

use std::{
    collections::BTreeMap,
    path::{
        Path,
        PathBuf,
    },
};

use serde::{
    Deserialize,
    Deserializer,
    de::Error as _,
};

use crate::{
    descriptor::Asciidoc,
    url::ExtensionStyle,
};

/// A parsed `antora-playbook.yml`, with every path still as written.
///
/// Relative paths in a playbook are relative to the playbook file, not to the
/// working directory — a playbook is meant to be runnable from anywhere in the
/// repository. [`resolve_paths`](Self::resolve_paths) applies that, and until
/// it has been called the paths are not usable.
#[derive(Clone, Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Playbook {
    /// What the site is called and where it lives.
    #[serde(default)]
    pub site: Site,

    /// Where the content comes from.
    #[serde(default)]
    pub content: Content,

    /// How URLs are shaped.
    #[serde(default)]
    pub urls: Urls,

    /// The UI to dress the pages in.
    #[serde(default)]
    pub ui: Ui,

    /// `AsciiDoc` settings applied to every page in the site.
    #[serde(default)]
    pub asciidoc: Asciidoc,

    /// Where the site is written.
    #[serde(default)]
    pub output: Output,

    /// Settings for the build itself rather than for the site.
    #[serde(default)]
    pub runtime: Runtime,

    /// Antora's own extension list. Read so that a playbook written for Antora
    /// still parses; the extensions themselves are Node modules and are not
    /// run.
    #[serde(default)]
    pub antora: Antora,

    /// How repositories are reached. Parsed so a real playbook loads; only the
    /// keys a local build needs are acted on.
    #[serde(default)]
    pub git: Git,

    /// Proxy settings, for a build that fetches. Parsed for the same reason.
    #[serde(default)]
    pub network: Network,
}

impl Playbook {
    /// Read a playbook from a file, resolving its relative paths against the
    /// file's own directory.
    pub fn load(path: &Path) -> Result<Self, LoadError> {
        let source = std::fs::read_to_string(path).map_err(|error| LoadError::Read {
            path: path.to_path_buf(),
            error,
        })?;

        let mut playbook: Self =
            serde_yaml_ng::from_str(&source).map_err(|error| LoadError::Parse {
                path: path.to_path_buf(),
                error: Box::new(error),
            })?;

        let base = path.parent().unwrap_or(Path::new("."));
        playbook.resolve_paths(base);

        Ok(playbook)
    }

    /// Rewrite every relative path in the playbook as relative to `base`.
    ///
    /// A content source URL is only a path when it is not a URL: `https://…`
    /// and `git@…` are left alone, and so is anything else with a scheme.
    pub fn resolve_paths(&mut self, base: &Path) {
        for source in &mut self.content.sources {
            if is_local(&source.url) {
                source.url = base.join(&source.url).to_string_lossy().into_owned();
            }
        }

        self.output.dir = base.join(&self.output.dir);

        if let Some(cache) = &self.runtime.cache_dir {
            self.runtime.cache_dir = Some(base.join(cache));
        }

        if let Some(bundle) = &mut self.ui.bundle
            && is_local(&bundle.url)
        {
            bundle.url = base.join(&bundle.url).to_string_lossy().into_owned();
        }
    }
}

/// Whether a content-source or UI-bundle `url:` names a place on this machine.
///
/// Anything with a `scheme://` prefix, and anything in `user@host:path` form,
/// is remote. Everything else is a path — including a bare `.`, which is how a
/// playbook names the repository it sits in.
pub fn is_local(url: &str) -> bool {
    if url.contains("://") {
        return false;
    }

    // `git@github.com:org/repo.git`: an `@` before the first `/`, with a `:`
    // after it. A Windows drive letter (`C:\…`) has no `@` and is unaffected.
    let head = url.split('/').next().unwrap_or(url);

    !(head.contains('@') && head.contains(':'))
}

/// Why a playbook could not be loaded.
#[derive(Debug, thiserror::Error)]
pub enum LoadError {
    /// The file could not be read.
    #[error("reading playbook `{path}`")]
    Read {
        /// The file that could not be read.
        path: PathBuf,

        /// What the file system said.
        #[source]
        error: std::io::Error,
    },

    /// The file was not a playbook.
    #[error("parsing playbook `{path}`")]
    Parse {
        /// The file that could not be parsed.
        path: PathBuf,

        /// What the YAML parser said.
        #[source]
        error: Box<serde_yaml_ng::Error>,
    },
}

/// The `site:` key.
#[derive(Clone, Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Site {
    /// The name shown in the navbar and appended to every page title.
    #[serde(default)]
    pub title: Option<String>,

    /// The site's base URL.
    ///
    /// Without one there are no canonical links and no sitemap: a page cannot
    /// say where it lives if the build does not know.
    #[serde(default)]
    pub url: Option<String>,

    /// The resource ID of the page `/` redirects to.
    #[serde(default)]
    pub start_page: Option<String>,

    /// What `robots.txt` should say: `allow`, `disallow`, or its literal
    /// contents.
    #[serde(default)]
    pub robots: Option<String>,

    /// Keys for whatever the UI wants to do with them — analytics IDs and the
    /// like. Carried through to the templates untouched.
    #[serde(default)]
    pub keys: BTreeMap<String, String>,

    /// Antora's deprecated spelling of `keys.google_analytics`. Read so a
    /// playbook that still has it loads.
    #[serde(default, rename = "__private__google_analytics_key")]
    pub google_analytics_key: Option<String>,
}

/// The `content:` key.
#[derive(Clone, Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Content {
    /// Where to collect content from.
    #[serde(default)]
    pub sources: Vec<Source>,

    /// The default `branches` for a source that names none.
    #[serde(default)]
    pub branches: Option<Refs>,

    /// The default `tags` for a source that names none.
    #[serde(default)]
    pub tags: Option<Refs>,

    /// The default `edit_url` for a source that names none.
    #[serde(default, deserialize_with = "optional_scalar")]
    pub edit_url: Option<String>,

    /// The default `version` for a source that names none.
    #[serde(default)]
    pub version: Option<VersionSpec>,

    /// The default `worktrees` for a source that names none.
    #[serde(default)]
    pub worktrees: Option<Worktrees>,
}

/// One entry of `content.sources`.
#[derive(Clone, Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Source {
    /// The repository: a URL, or a path on this machine.
    pub url: String,

    /// Which branches to read. `HEAD` means the current worktree.
    #[serde(default)]
    pub branches: Option<Refs>,

    /// Which tags to read.
    #[serde(default)]
    pub tags: Option<Refs>,

    /// A single directory within the repository that holds the `antora.yml`.
    #[serde(default)]
    pub start_path: Option<String>,

    /// Several such directories, for a repository holding more than one
    /// component version.
    #[serde(default)]
    pub start_paths: Vec<String>,

    /// The template for the "Edit this page" link, with `{web_url}`,
    /// `{refname}` and `{path}` placeholders. `false` switches the link
    /// off.
    #[serde(default, deserialize_with = "optional_scalar")]
    pub edit_url: Option<String>,

    /// Paths within the source not to collect.
    #[serde(default)]
    pub exclude: Option<Refs>,

    /// What version each ref of this source contributes.
    ///
    /// This is a *fallback*: a component descriptor that names its own version
    /// still wins. It is how one `antora.yml`, committed to several branches,
    /// describes several versions without being edited on each.
    #[serde(default)]
    pub version: Option<VersionSpec>,

    /// Which refs are read from a worktree rather than from git's object
    /// database.
    #[serde(default)]
    pub worktrees: Option<Worktrees>,
}

impl Source {
    /// Every start path this source names, as at least one entry.
    ///
    /// A source that names none still has one — the repository root — so
    /// callers never have to handle "no paths" separately from "one path".
    pub fn start_paths(&self) -> Vec<String> {
        if !self.start_paths.is_empty() {
            return self.start_paths.clone();
        }

        vec![self.start_path.clone().unwrap_or_default()]
    }

    /// Which branches this source reads, falling back to the playbook's
    /// default and then to Antora's.
    pub fn branches(&self, content: &Content) -> Vec<String> {
        self.branches
            .as_ref()
            .or(content.branches.as_ref())
            .map_or_else(default_branches, Refs::patterns)
    }

    /// Which tags this source reads. Antora reads none unless asked.
    pub fn tags(&self, content: &Content) -> Vec<String> {
        self.tags
            .as_ref()
            .or(content.tags.as_ref())
            .map_or_else(Vec::new, Refs::patterns)
    }

    /// What version this source's refs contribute, when the descriptor does not
    /// say.
    pub fn version<'a>(&'a self, content: &'a Content) -> Option<&'a VersionSpec> {
        self.version.as_ref().or(content.version.as_ref())
    }

    /// Which refs are read from a worktree.
    pub fn worktrees<'a>(&'a self, content: &'a Content) -> &'a Worktrees {
        self.worktrees
            .as_ref()
            .or(content.worktrees.as_ref())
            .unwrap_or(&Worktrees::CURRENT)
    }
}

/// What Antora reads when a playbook names no branches: the current worktree,
/// and every tag-shaped branch.
fn default_branches() -> Vec<String> {
    vec!["HEAD".to_string(), "v{0..9}*".to_string()]
}

/// A `branches:`/`tags:`/`exclude:` value, which YAML lets be one string or a
/// list of them.
#[derive(Clone, Debug, Deserialize)]
#[serde(untagged)]
pub enum Refs {
    /// A single pattern.
    One(String),

    /// Several patterns.
    Many(Vec<String>),
}

impl Refs {
    /// The patterns, however they were written.
    pub fn patterns(&self) -> Vec<String> {
        match self {
            Self::One(pattern) => vec![pattern.clone()],
            Self::Many(patterns) => patterns.clone(),
        }
    }
}

/// What version a source's refs contribute, when the component descriptor does
/// not say.
///
/// This exists because one `antora.yml`, committed to several branches, is the
/// normal way to describe several versions of a component — and a descriptor
/// that named its own version would have to be edited on every branch, which is
/// exactly the merge conflict nobody wants.
#[derive(Clone, Debug, Deserialize)]
#[serde(untagged)]
pub enum VersionSpec {
    /// `version: true` uses the ref's own name; `version: false` makes the
    /// component unversioned.
    Flag(bool),

    /// A version for every ref this source reads.
    Text(String),

    /// A number.
    ///
    /// Kept as YAML read it rather than as an `f64`, because `2.0` and `2` are
    /// different versions and a round trip through a float loses the
    /// difference — a `version: 2.0` published at `/2/` would break every link
    /// anyone had to it.
    Number(serde_yaml_ng::Number),

    /// Patterns matched against the ref name, each with what to call it.
    ///
    /// A pattern may be a name (`main`), or a glob with a capture
    /// (`v{0..9}*` → `*`), and the first that matches wins. A ref that matches
    /// nothing keeps its own name.
    Patterns(BTreeMap<String, Option<String>>),
}

impl VersionSpec {
    /// What `refname` should be called.
    ///
    /// `None` means this spec says nothing and the descriptor must.
    pub fn version_of(&self, refname: &str) -> Option<String> {
        let version = match self {
            // The ref's own name, with the separators a URL cannot carry
            // replaced — `release/2.0` becomes `release-2.0`.
            Self::Flag(true) => refname.to_string(),

            // Unversioned, which is how a single-branch component says it has
            // no versions rather than one called `main`.
            Self::Flag(false) => String::new(),

            Self::Text(text) => text.clone(),

            Self::Number(number) => match number.as_i64() {
                Some(integer) => integer.to_string(),
                None => number.to_string(),
            },

            Self::Patterns(patterns) => match patterns.get(refname) {
                Some(replacement) => replacement.clone().unwrap_or_default(),

                None => patterns
                    .iter()
                    .find_map(|(pattern, replacement)| {
                        substitute(refname, pattern, replacement.as_deref())
                    })
                    // A ref that matches no pattern keeps its own name, which
                    // is what Antora does and means a new branch appears under
                    // its own name rather than not at all.
                    .unwrap_or_else(|| refname.to_string()),
            },
        };

        Some(version.replace(['/', '\\'], "-"))
    }
}

/// Apply one `version:` pattern to a ref name.
///
/// A pattern is a glob, and `*` in the replacement stands for what the glob's
/// own `*` matched — so `v{0..9}*` → `*` turns `v2.1` into `2.1`. A replacement
/// with no `*` is used as it stands.
fn substitute(refname: &str, pattern: &str, replacement: Option<&str>) -> Option<String> {
    let captured = glob_capture(refname, pattern)?;

    Some(match replacement {
        None => String::new(),
        Some(replacement) => replacement.replace('*', &captured),
    })
}

/// Match `refname` against a glob, returning what its `*` matched.
///
/// The globs a `version:` key uses are small — a literal prefix, a `{0..9}`
/// digit class, and a trailing `*` — so they are matched directly rather than
/// compiled. Anything more elaborate falls through as "no match", which leaves
/// the ref under its own name rather than under a wrong one.
fn glob_capture(refname: &str, pattern: &str) -> Option<String> {
    let mut rest = refname;
    let mut captured = None;
    let mut chars = pattern.chars().peekable();

    while let Some(character) = chars.next() {
        match character {
            '*' => {
                // A trailing `*` takes everything that is left; one in the
                // middle is more than these patterns are meant to express.
                if chars.peek().is_some() {
                    return None;
                }

                captured = Some(rest.to_string());
                rest = "";
            }

            '{' => {
                // `{0..9}` — one character from a range.
                let class: String = chars.by_ref().take_while(|c| *c != '}').collect();
                let (low, high) = class.split_once("..")?;

                let low = low.chars().next()?;
                let high = high.chars().next()?;
                let next = rest.chars().next()?;

                if next < low || next > high {
                    return None;
                }

                rest = &rest[next.len_utf8()..];
            }

            literal => {
                rest = rest.strip_prefix(literal)?;
            }
        }
    }

    rest.is_empty()
        .then(|| captured.unwrap_or_else(|| refname.to_string()))
}

/// Which of a source's refs are read from a worktree rather than from git.
#[derive(Clone, Debug, Deserialize)]
#[serde(untagged)]
pub enum Worktrees {
    /// `true` reads every ref that has a worktree; `false` reads none.
    Flag(bool),

    /// A ref name, or `.` for whichever the repository has checked out.
    One(String),

    /// Several of those.
    Many(Vec<String>),
}

impl Worktrees {
    /// Antora's default: the checked-out worktree, and nothing else.
    pub const CURRENT: Self = Self::Flag(true);

    /// Whether `refname` should be read from a worktree.
    ///
    /// `.` means "whichever ref is checked out", which is why `current` has to
    /// be supplied rather than compared against a name.
    pub fn includes(&self, refname: &str, current: Option<&str>) -> bool {
        let names = match self {
            Self::Flag(flag) => return *flag && current == Some(refname),
            Self::One(name) => std::slice::from_ref(name),
            Self::Many(names) => names.as_slice(),
        };

        names.iter().any(|name| {
            if name == "." {
                current == Some(refname)
            } else {
                name == refname
            }
        })
    }
}

/// The `git:` key: how repositories are reached.
#[derive(Clone, Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Git {
    /// Whether a URL with no `.git` suffix gets one.
    #[serde(default)]
    pub ensure_git_suffix: Option<bool>,

    /// Where credentials for private repositories come from.
    #[serde(default)]
    pub credentials: Option<serde_yaml_ng::Value>,

    /// How many repositories are fetched at once.
    #[serde(default)]
    pub fetch_concurrency: Option<u32>,

    /// How deep a fetch goes.
    #[serde(default)]
    pub fetch_depth: Option<u32>,

    /// How many refs are read at once.
    #[serde(default)]
    pub read_concurrency: Option<u32>,

    /// Antora's git plugins. Read so a real playbook parses; they are Node
    /// modules and are not loaded.
    #[serde(default)]
    pub plugins: Option<serde_yaml_ng::Value>,
}

/// The `network:` key.
#[derive(Clone, Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Network {
    /// The proxy for `http://` URLs.
    #[serde(default)]
    pub http_proxy: Option<String>,

    /// The proxy for `https://` URLs.
    #[serde(default)]
    pub https_proxy: Option<String>,

    /// Hosts that bypass the proxy.
    #[serde(default)]
    pub no_proxy: Option<String>,
}

/// The `urls:` key.
#[derive(Clone, Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Urls {
    /// How a page's `.adoc` becomes a URL.
    #[serde(default, deserialize_with = "extension_style")]
    pub html_extension_style: ExtensionStyle,

    /// How redirects for aliases are published.
    #[serde(default)]
    pub redirect_facility: Option<String>,

    /// An extra URL segment that always points at the latest version, e.g.
    /// `latest`.
    #[serde(default, deserialize_with = "optional_scalar")]
    pub latest_version_segment: Option<String>,

    /// The same for the latest prerelease.
    #[serde(default, deserialize_with = "optional_scalar")]
    pub latest_prerelease_version_segment: Option<String>,

    /// Which versions the segment strategy applies to.
    #[serde(default)]
    pub latest_version_segment_strategy: Option<String>,
}

/// The `ui:` key.
#[derive(Clone, Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Ui {
    /// The UI archive to unpack over the site.
    ///
    /// Left out, the UI built into this crate is used, which is why a playbook
    /// here needs no network to build.
    #[serde(default)]
    pub bundle: Option<Bundle>,

    /// The layout used by a page that names none.
    #[serde(default)]
    pub default_layout: Option<String>,

    /// Where the UI's own files go, relative to the output directory.
    #[serde(default)]
    pub output_dir: Option<String>,

    /// Files layered over the UI after it is unpacked.
    #[serde(default)]
    pub supplemental_files: Option<Supplemental>,
}

/// `ui.supplemental_files:` — what to lay over the UI.
#[derive(Clone, Debug, Deserialize)]
#[serde(untagged)]
pub enum Supplemental {
    /// A directory whose contents are copied over the UI.
    Directory(String),

    /// Individual files, each named and sourced.
    Files(Vec<SupplementalFile>),
}

/// One entry of `ui.supplemental_files:`.
#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SupplementalFile {
    /// Where the file goes, relative to the UI output directory.
    pub path: String,

    /// Where it comes from: a path to read, or the contents themselves.
    ///
    /// Antora tells the two apart by whether the value names a file that
    /// exists, which is why one key carries both.
    #[serde(default)]
    pub contents: Option<String>,
}

/// The `ui.bundle:` key.
#[derive(Clone, Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Bundle {
    /// The archive: a URL, or a path on this machine.
    pub url: String,

    /// Whether the archive at that URL changes without its URL changing, and
    /// so must be re-fetched rather than cached forever.
    #[serde(default)]
    pub snapshot: bool,

    /// A directory within the archive to treat as its root.
    #[serde(default)]
    pub start_path: Option<String>,
}

/// The `output:` key.
#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Output {
    /// Where the site is written.
    #[serde(default = "default_output_dir")]
    pub dir: PathBuf,

    /// Whether the output directory is emptied first.
    #[serde(default)]
    pub clean: bool,

    /// Antora's multi-destination output. Parsed but not acted on.
    #[serde(default)]
    pub destinations: Vec<serde_yaml_ng::Value>,
}

impl Default for Output {
    fn default() -> Self {
        Self {
            dir: default_output_dir(),
            clean: false,
            destinations: Vec::new(),
        }
    }
}

/// Antora's default, and the one a playbook that says nothing gets.
fn default_output_dir() -> PathBuf {
    PathBuf::from("build/site")
}

/// The `runtime:` key.
#[derive(Clone, Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
#[expect(
    clippy::struct_excessive_bools,
    reason = "these mirror the playbook's own keys, one field each"
)]
pub struct Runtime {
    /// Where fetched repositories and UI bundles are kept.
    #[serde(default)]
    pub cache_dir: Option<PathBuf>,

    /// Whether remote sources are re-fetched.
    #[serde(default)]
    pub fetch: bool,

    /// Suppress progress output.
    #[serde(default)]
    pub quiet: bool,

    /// Suppress all output.
    #[serde(default)]
    pub silent: bool,

    /// Whether a failure prints where it came from.
    #[serde(default)]
    pub stacktrace: bool,

    /// How much to log, and what makes a build fail.
    #[serde(default)]
    pub log: Log,
}

/// The `runtime.log:` key.
#[derive(Clone, Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Log {
    /// The lowest level to report: `all`, `debug`, `info`, `warn`, `error`,
    /// `fatal`, or `silent`.
    #[serde(default)]
    pub level: Option<String>,

    /// The level at which a reported problem fails the build.
    #[serde(default)]
    pub failure_level: Option<String>,

    /// `pretty` or `json`.
    #[serde(default)]
    pub format: Option<String>,

    /// Where the log goes when not the terminal.
    #[serde(default)]
    pub destination: Option<serde_yaml_ng::Value>,

    /// Antora's older spelling of `destination.file`.
    #[serde(default)]
    pub file: Option<String>,
}

/// The `antora:` key: Antora's own extension points.
#[derive(Clone, Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Antora {
    /// The extensions a playbook asks Antora to load. Recorded so a playbook
    /// written for Antora still parses; they are Node modules and are not run.
    #[serde(default)]
    pub extensions: Vec<serde_yaml_ng::Value>,

    /// The site generator Antora should use. Parsed and ignored: this *is* the
    /// site generator.
    #[serde(default)]
    pub generator: Option<String>,
}

impl Antora {
    /// What each extension is required by name, for reporting them.
    pub fn extension_names(&self) -> Vec<String> {
        self.extensions
            .iter()
            .map(|extension| match extension {
                serde_yaml_ng::Value::String(name) => name.clone(),

                serde_yaml_ng::Value::Mapping(map) => map
                    .get(serde_yaml_ng::Value::String("require".to_string()))
                    .and_then(serde_yaml_ng::Value::as_str)
                    .unwrap_or("<unnamed>")
                    .to_string(),

                _ => "<unnamed>".to_string(),
            })
            .collect()
    }
}

/// Read an `html_extension_style:` value.
fn extension_style<'de, D: Deserializer<'de>>(de: D) -> Result<ExtensionStyle, D::Error> {
    let Some(style) = Option::<String>::deserialize(de)? else {
        return Ok(ExtensionStyle::Default);
    };

    match style.as_str() {
        "default" => Ok(ExtensionStyle::Default),
        "drop" => Ok(ExtensionStyle::Drop),
        "indexify" => Ok(ExtensionStyle::Indexify),

        other => Err(D::Error::custom(format!(
            "`{other}` is not an html_extension_style: expected `default`, `drop` or `indexify`"
        ))),
    }
}

/// Read a YAML scalar of any type as an optional string.
///
/// A version written `2.0` is a float and one written `2` is an integer, but
/// both are meant as the text that goes in a URL. An `edit_url: false` is a
/// boolean meaning "no link". Reading these as strings up front keeps the rest
/// of the model from having to know what YAML made of them.
pub fn optional_scalar<'de, D: Deserializer<'de>>(de: D) -> Result<Option<String>, D::Error> {
    let Some(value) = Option::<serde_yaml_ng::Value>::deserialize(de)? else {
        return Ok(None);
    };

    Ok(match value {
        // `~` and `false` both mean "no value": a `version: ~` is an
        // unversioned component and an `edit_url: false` is no link.
        serde_yaml_ng::Value::Null | serde_yaml_ng::Value::Bool(false) => None,
        serde_yaml_ng::Value::Bool(true) => Some(String::new()),
        serde_yaml_ng::Value::String(text) => Some(text),

        serde_yaml_ng::Value::Number(number) => Some(match number.as_i64() {
            Some(integer) => integer.to_string(),
            None => number.to_string(),
        }),

        other => {
            return Err(D::Error::custom(format!(
                "expected a scalar, found `{other:?}`"
            )));
        }
    })
}

#[cfg(test)]
mod tests {
    #![expect(clippy::unwrap_used, reason = "a failed parse is the test failing")]

    use super::*;

    #[test]
    fn a_minimal_playbook_parses() {
        let playbook: Playbook =
            serde_yaml_ng::from_str("site:\n  title: A Site\ncontent:\n  sources:\n    - url: .\n")
                .unwrap();

        assert_eq!(playbook.site.title.as_deref(), Some("A Site"));
        assert_eq!(playbook.content.sources.len(), 1);
        assert_eq!(playbook.output.dir, PathBuf::from("build/site"));
    }

    #[test]
    fn a_source_always_has_at_least_one_start_path() {
        let source = Source {
            url: ".".to_string(),
            ..Source::default()
        };

        assert_eq!(source.start_paths(), vec![String::new()]);
    }

    #[test]
    fn start_paths_wins_over_start_path() {
        let source: Source =
            serde_yaml_ng::from_str("url: .\nstart_path: one\nstart_paths: [two, three]\n")
                .unwrap();

        assert_eq!(source.start_paths(), vec!["two", "three"]);
    }

    #[test]
    fn branches_accept_one_value_or_many() {
        let one: Source = serde_yaml_ng::from_str("url: .\nbranches: HEAD\n").unwrap();
        let many: Source = serde_yaml_ng::from_str("url: .\nbranches: [main, v*]\n").unwrap();

        assert_eq!(one.branches.unwrap().patterns(), vec!["HEAD"]);
        assert_eq!(many.branches.unwrap().patterns(), vec!["main", "v*"]);
    }

    #[test]
    fn local_urls_are_told_from_remote_ones() {
        assert!(is_local("."));
        assert!(is_local("../.."));
        assert!(is_local("/srv/docs"));
        assert!(is_local("./docs"));

        assert!(!is_local("https://example.org/repo.git"));
        assert!(!is_local("git@github.com:org/repo.git"));
        assert!(!is_local("file:///srv/docs"));
    }

    #[test]
    fn relative_paths_resolve_against_the_playbook() {
        let mut playbook: Playbook = serde_yaml_ng::from_str(
            "content:\n  sources:\n    - url: ../..\n    - url: https://example.org/r.git\noutput:\n  dir: \
             ./build/antora\n",
        )
        .unwrap();

        playbook.resolve_paths(Path::new("/repo/docs"));

        assert_eq!(playbook.content.sources[0].url, "/repo/docs/../..");
        assert_eq!(playbook.content.sources[1].url, "https://example.org/r.git");
        assert_eq!(
            playbook.output.dir,
            PathBuf::from("/repo/docs/./build/antora")
        );
    }

    #[test]
    fn an_unknown_extension_style_is_rejected_by_name() {
        let error = serde_yaml_ng::from_str::<Urls>("html_extension_style: pretty\n")
            .unwrap_err()
            .to_string();

        assert!(error.contains("pretty"), "{error}");
    }
}
