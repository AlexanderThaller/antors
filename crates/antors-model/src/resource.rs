//! Resource IDs: how Antora names every file in a site.
//!
//! Every file Antora publishes has an identifier of five parts:
//!
//! ```text
//! version@component:module:family$relative/path.ext
//! ```
//!
//! Only the relative path is required. Each omitted part is taken from the
//! page the reference was written in, which is what makes a reference to a
//! sibling page as short as `xref:other.adoc[]` while a reference across a
//! whole site is still one string.
//!
//! The rules for what an omitted part means are not symmetric, and the
//! asymmetry is the thing to get right:
//!
//! - Naming a *component* without a *module* is not allowed — `component::`
//!   (with the empty module) is how you say "that component's `ROOT`". A lone
//!   `name:` is a module, not a component, because references within one
//!   component are the common case and get the shorter form.
//! - Naming a component without a version means *its latest version*, not the
//!   referring page's version. A `2.0` page linking to `other::index.adoc`
//!   wants whatever `other` currently ships, not a `2.0` of `other` that may
//!   not exist.
//! - The family defaults to whatever the *macro* implies, not to `page`:
//!   `xref:` means a page, `image::` means an image, `include::` means a
//!   partial or an example depending on what it says.
//!
//! Filling in the version needs the whole site, so it is deliberately not done
//! here: [`ResourceId::resolve_in`] returns a [`ResolvedId`] whose version is
//! still optional, and the catalog that knows which versions exist settles it.

use std::{
    fmt,
    str::FromStr,
};

/// The families a resource can belong to.
///
/// A family is a directory under a module — `pages`, `partials`, `examples`,
/// `images`, `attachments` — and it decides two things at once: where the file
/// was read from, and what may be done with it. Only a page is published as a
/// page; only an image and an attachment are published at all; a partial and
/// an example exist solely to be included.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum Family {
    /// `attachments/` — published under `_attachments`, linked but not
    /// rendered.
    Attachment,

    /// `examples/` — included into a page, never published.
    Example,

    /// `images/` — published under `_images`.
    Image,

    /// `pages/` — rendered and published as HTML.
    Page,

    /// `partials/` — included into a page, never published.
    Partial,

    /// A navigation file named by the component descriptor.
    ///
    /// It has no directory of its own: a `nav.adoc` is listed by path in
    /// `antora.yml` and may sit anywhere in the module. It is never published.
    Nav,

    /// A page that used to live at this ID, published as a redirect to the one
    /// that now does.
    ///
    /// Nothing on disk is in this family; it exists because a `page-aliases`
    /// entry occupies a resource ID and must not silently collide with a real
    /// page at the same one.
    Alias,
}

impl Family {
    /// The `name$` prefix that names this family in a resource ID.
    ///
    /// [`Nav`](Self::Nav) and [`Alias`](Self::Alias) have none: neither can be
    /// written in a reference, because neither is a file an author addresses.
    pub fn prefix(self) -> Option<&'static str> {
        match self {
            Self::Attachment => Some("attachment"),
            Self::Example => Some("example"),
            Self::Image => Some("image"),
            Self::Page => Some("page"),
            Self::Partial => Some("partial"),
            Self::Nav | Self::Alias => None,
        }
    }

    /// The directory under a module that holds this family's files.
    pub fn directory(self) -> Option<&'static str> {
        match self {
            Self::Attachment => Some("attachments"),
            Self::Example => Some("examples"),
            Self::Image => Some("images"),
            Self::Page => Some("pages"),
            Self::Partial => Some("partials"),
            Self::Nav | Self::Alias => None,
        }
    }

    /// Whether files in this family are written to the site.
    ///
    /// A partial and an example are read during a build and then have nothing
    /// further to do with it; publishing them would put a half-document at a
    /// URL and invite a search engine to index it.
    pub fn is_published(self) -> bool {
        matches!(
            self,
            Self::Page | Self::Image | Self::Attachment | Self::Alias
        )
    }

    /// The directory a published non-page family is written into, relative to
    /// its module's root.
    ///
    /// The leading underscore is Antora's, and it is load-bearing: it keeps
    /// these directories out of the namespace a page's own URL lives in, so a
    /// page may be called `images.adoc` without colliding with the images.
    pub fn output_directory(self) -> Option<&'static str> {
        match self {
            Self::Image => Some("_images"),
            Self::Attachment => Some("_attachments"),
            _ => None,
        }
    }

    /// Read a `name$` prefix, without its `$`.
    pub fn from_prefix(prefix: &str) -> Option<Self> {
        match prefix {
            "attachment" => Some(Self::Attachment),
            "example" => Some(Self::Example),
            "image" => Some(Self::Image),
            "page" => Some(Self::Page),
            "partial" => Some(Self::Partial),
            _ => None,
        }
    }

    /// Read a family directory name.
    pub fn from_directory(directory: &str) -> Option<Self> {
        match directory {
            "attachments" => Some(Self::Attachment),
            "examples" => Some(Self::Example),
            "images" => Some(Self::Image),
            "pages" => Some(Self::Page),
            "partials" => Some(Self::Partial),
            _ => None,
        }
    }
}

impl fmt::Display for Family {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let name = match self {
            Self::Attachment => "attachment",
            Self::Example => "example",
            Self::Image => "image",
            Self::Page => "page",
            Self::Partial => "partial",
            Self::Nav => "nav",
            Self::Alias => "alias",
        };

        f.write_str(name)
    }
}

/// A resource ID as an author wrote it, with the parts they left out still
/// left out.
///
/// This is the parsed form of the string, not a resolved address: every field
/// but [`relative`](Self::relative) may be absent, and what an absent field
/// means depends on the page the reference was written in. Call
/// [`resolve_in`](Self::resolve_in) to apply that context.
#[derive(Clone, Debug, Default, Eq, Hash, PartialEq)]
pub struct ResourceId {
    /// The component version, from a `version@` prefix.
    pub version: Option<String>,

    /// The component name.
    pub component: Option<String>,

    /// The module name. `Some("")` is how `component::path` says `ROOT`, and
    /// is normalized to `ROOT` during resolution.
    pub module: Option<String>,

    /// The family, from a `family$` prefix.
    pub family: Option<Family>,

    /// The path within the family directory, always with `/` separators.
    pub relative: String,
}

/// Why a string is not a resource ID.
#[derive(Clone, Debug, Eq, PartialEq, thiserror::Error)]
pub enum ParseError {
    /// The string was empty, or had an empty path after its prefixes.
    #[error("a resource ID needs a path")]
    NoPath,

    /// A `name$` prefix named a family that does not exist.
    #[error("`{0}` is not a resource family")]
    UnknownFamily(String),

    /// More than three `:`-separated segments before the path.
    #[error("a resource ID has at most `component:module:path`, found {0} segments")]
    TooManySegments(usize),
}

impl ResourceId {
    /// Parse a resource ID.
    ///
    /// The grammar is positional rather than tagged, so the count of
    /// `:`-separated segments is what decides their meaning: one is a path,
    /// two are a module and a path, three are a component, a module and a
    /// path. This is why `component:path` cannot be written — it is read as a
    /// module — and why `component::path` is the way to reach another
    /// component's `ROOT`.
    pub fn parse(spec: &str) -> Result<Self, ParseError> {
        let spec = spec.trim();

        // The version is separated by `@`, which cannot appear anywhere else
        // in an ID — so the first one, if any, is the separator.
        let (version, rest) = match spec.split_once('@') {
            Some((version, rest)) if !version.is_empty() => (Some(version.to_string()), rest),
            _ => (None, spec),
        };

        let segments: Vec<&str> = rest.split(':').collect();

        let (component, module, tail) = match segments.as_slice() {
            [tail] => (None, None, *tail),
            [module, tail] => (None, Some((*module).to_string()), *tail),

            [component, module, tail] => (
                Some((*component).to_string()),
                Some((*module).to_string()),
                *tail,
            ),

            other => return Err(ParseError::TooManySegments(other.len())),
        };

        // The family sits immediately before the path, so it is the last
        // prefix to come off.
        let (family, relative) = match tail.split_once('$') {
            Some((prefix, relative)) => {
                let family = Family::from_prefix(prefix)
                    .ok_or_else(|| ParseError::UnknownFamily(prefix.to_string()))?;

                (Some(family), relative)
            }

            None => (None, tail),
        };

        if relative.is_empty() {
            return Err(ParseError::NoPath);
        }

        Ok(Self {
            version,
            component,
            module,
            family,
            relative: relative.to_string(),
        })
    }

    /// Whether this ID names anything outside the page it was written in.
    ///
    /// A reference that names nothing — no version, component, module or
    /// family — is a plain relative path, which some macros are allowed to
    /// treat as a path on disk rather than as a resource ID.
    pub fn is_bare(&self) -> bool {
        self.version.is_none()
            && self.component.is_none()
            && self.module.is_none()
            && self.family.is_none()
    }

    /// Fill in what the author left out from the page the reference sits in.
    ///
    /// `default_family` is what the *macro* implies: a page for `xref:`, an
    /// image for `image::`. The version is deliberately left alone when the
    /// reference names another component — only the catalog knows which of its
    /// versions is the latest.
    pub fn resolve_in(&self, context: &Context, default_family: Family) -> ResolvedId {
        let component = self
            .component
            .clone()
            .unwrap_or_else(|| context.component.clone());

        // Another component's latest version is almost never the referring
        // page's version, and is often not a version that component has at
        // all. Leaving it unset says "ask the catalog".
        let version = self
            .version
            .clone()
            .or_else(|| (component == context.component).then(|| context.version.clone()));

        let module = match &self.module {
            // `component::path` — the empty module is how ROOT is written.
            Some(module) if module.is_empty() => ROOT_MODULE.to_string(),
            Some(module) => module.clone(),

            // Naming a component without a module means its ROOT, not the
            // referring page's module: `other:` and this page's `guide` have
            // nothing to do with each other.
            None if self.component.is_some() => ROOT_MODULE.to_string(),
            None => context.module.clone(),
        };

        ResolvedId {
            component,
            version,
            module,
            family: self.family.unwrap_or(default_family),
            relative: self.relative.clone(),
        }
    }
}

impl FromStr for ResourceId {
    type Err = ParseError;

    fn from_str(spec: &str) -> Result<Self, Self::Err> {
        Self::parse(spec)
    }
}

impl fmt::Display for ResourceId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if let Some(version) = &self.version {
            write!(f, "{version}@")?;
        }

        if let Some(component) = &self.component {
            write!(f, "{component}:")?;
        }

        if let Some(module) = &self.module {
            write!(f, "{module}:")?;
        }

        if let Some(prefix) = self.family.and_then(Family::prefix) {
            write!(f, "{prefix}$")?;
        }

        f.write_str(&self.relative)
    }
}

/// The module every component has, and the one a bare reference means.
pub const ROOT_MODULE: &str = "ROOT";

/// Where a reference was written, so that what it left out can be filled in.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct Context {
    /// The component the referring page belongs to.
    pub component: String,

    /// The version of that component. Empty for an unversioned component.
    pub version: String,

    /// The module the referring page belongs to.
    pub module: String,
}

/// A resource ID with every part supplied but the version, which only the
/// catalog can settle.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct ResolvedId {
    /// The component name.
    pub component: String,

    /// The version, or `None` for "whichever is latest".
    pub version: Option<String>,

    /// The module name.
    pub module: String,

    /// The family.
    pub family: Family,

    /// The path within the family.
    pub relative: String,
}

/// A resource ID with nothing left to work out: the key a catalog is indexed
/// by.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct Key {
    /// The component name.
    pub component: String,

    /// The version. Empty for an unversioned component.
    pub version: String,

    /// The module name.
    pub module: String,

    /// The family.
    pub family: Family,

    /// The path within the family.
    pub relative: String,
}

impl fmt::Display for Key {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}@{}:{}:", self.version, self.component, self.module)?;

        if let Some(prefix) = self.family.prefix() {
            write!(f, "{prefix}$")?;
        }

        f.write_str(&self.relative)
    }
}

#[cfg(test)]
mod tests {
    #![expect(clippy::unwrap_used, reason = "a failed parse is the test failing")]

    use super::*;

    fn context() -> Context {
        Context {
            component: "showcase".to_string(),
            version: "2.0".to_string(),
            module: "guide".to_string(),
        }
    }

    #[test]
    fn bare_path_takes_everything_from_context() {
        let id = ResourceId::parse("page.adoc").unwrap();
        assert!(id.is_bare());

        let resolved = id.resolve_in(&context(), Family::Page);
        assert_eq!(resolved.component, "showcase");
        assert_eq!(resolved.version.as_deref(), Some("2.0"));
        assert_eq!(resolved.module, "guide");
        assert_eq!(resolved.family, Family::Page);
    }

    #[test]
    fn one_segment_prefix_is_a_module_not_a_component() {
        let resolved = ResourceId::parse("api:errors.adoc")
            .unwrap()
            .resolve_in(&context(), Family::Page);

        assert_eq!(resolved.component, "showcase");
        assert_eq!(resolved.module, "api");
    }

    #[test]
    fn empty_module_means_root() {
        let resolved = ResourceId::parse("sidecar::index.adoc")
            .unwrap()
            .resolve_in(&context(), Family::Page);

        assert_eq!(resolved.component, "sidecar");
        assert_eq!(resolved.module, ROOT_MODULE);
    }

    #[test]
    fn another_component_has_no_version_of_its_own() {
        // The referring page's `2.0` says nothing about `sidecar`, so the
        // version is left for the catalog to fill in with the latest.
        let resolved = ResourceId::parse("sidecar::index.adoc")
            .unwrap()
            .resolve_in(&context(), Family::Page);

        assert_eq!(resolved.version, None);
    }

    #[test]
    fn version_prefix_is_kept() {
        let resolved = ResourceId::parse("1.0@showcase:ROOT:legacy.adoc")
            .unwrap()
            .resolve_in(&context(), Family::Page);

        assert_eq!(resolved.version.as_deref(), Some("1.0"));
        assert_eq!(resolved.component, "showcase");
        assert_eq!(resolved.module, "ROOT");
        assert_eq!(resolved.relative, "legacy.adoc");
    }

    #[test]
    fn version_without_a_component_stays_in_this_one() {
        let resolved = ResourceId::parse("1.0@legacy.adoc")
            .unwrap()
            .resolve_in(&context(), Family::Page);

        assert_eq!(resolved.component, "showcase");
        assert_eq!(resolved.module, "guide");
        assert_eq!(resolved.version.as_deref(), Some("1.0"));
    }

    #[test]
    fn family_prefix_overrides_the_macro() {
        let resolved = ResourceId::parse("attachment$report.pdf")
            .unwrap()
            .resolve_in(&context(), Family::Page);

        assert_eq!(resolved.family, Family::Attachment);
    }

    #[test]
    fn family_prefix_combines_with_a_module() {
        let id = ResourceId::parse("ROOT:partial$shared.adoc").unwrap();

        assert_eq!(id.module.as_deref(), Some("ROOT"));
        assert_eq!(id.family, Some(Family::Partial));
        assert_eq!(id.relative, "shared.adoc");
    }

    #[test]
    fn an_unknown_family_is_an_error() {
        assert_eq!(
            ResourceId::parse("snippet$x.adoc"),
            Err(ParseError::UnknownFamily("snippet".to_string()))
        );
    }

    #[test]
    fn a_path_is_required() {
        assert_eq!(ResourceId::parse("partial$"), Err(ParseError::NoPath));
    }

    #[test]
    fn round_trips_through_display() {
        for spec in [
            "page.adoc",
            "api:errors.adoc",
            "sidecar::index.adoc",
            "1.0@showcase:ROOT:legacy.adoc",
            "attachment$report.pdf",
            "ROOT:partial$shared.adoc",
            "sub/dir/page.adoc",
        ] {
            assert_eq!(ResourceId::parse(spec).unwrap().to_string(), spec);
        }
    }
}
