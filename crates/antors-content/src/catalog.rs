//! The catalog: every resource in the site, addressable by its ID.
//!
//! The catalog is the build's single index. A reference written in a page is
//! resolved against it, an `include::` is read through it, a navigation entry
//! is checked against it, and the URL a page is published at is its answer —
//! so there is exactly one place that knows what the site contains, and one
//! place that decides where anything lives.

use std::{
    collections::{
        BTreeMap,
        btree_map::Entry,
    },
    sync::Arc,
};

use crate::contents::Contents;
use antors_model::{
    descriptor::Descriptor,
    resource::{
        Family,
        Key,
        ResolvedId,
    },
    url::{
        ExtensionStyle,
        Location,
        locate,
    },
    version,
};

use crate::origin::Origin;

/// One file collected from a content source.
#[derive(Clone, Debug)]
pub struct SourceFile {
    /// What the file is addressed by.
    pub key: Key,

    /// Where it came from.
    pub origin: Arc<Origin>,

    /// Its bytes, or the way to get them.
    pub contents: Contents,

    /// Its path within the component version, from the `antora.yml` down —
    /// what `page-relative-src-path` reports and what an edit link is built
    /// from.
    pub relative_src_path: String,

    /// Where it is published, or `None` for a family that is not.
    pub location: Option<Location>,

    /// The page's title, once the page has been parsed.
    ///
    /// A reference with no text of its own shows this, so it has to be known
    /// before *any* page's references are resolved — which is why parsing a
    /// site happens in two passes rather than one.
    pub title: Option<String>,

    /// What the navigation shows for this page, from its `navtitle` attribute.
    pub nav_title: Option<String>,

    /// The page this one redirects to, for an alias.
    ///
    /// An alias has no content of its own: it is an ID a page used to answer
    /// to, published as a redirect. Which page it now points at cannot be
    /// worked out from the alias's own ID — the page may be in another module
    /// entirely — so it is recorded when the alias is created.
    pub redirect_to: Option<Key>,
}

impl SourceFile {
    /// The URL this file is published at, or `None` if it is not.
    pub fn url(&self) -> Option<&str> {
        self.location.as_ref().map(|location| location.url.as_str())
    }

    /// How to name this file in a message.
    pub fn describe(&self) -> String {
        self.contents.describe(&self.key.to_string())
    }

    /// What a reference with no text of its own should show.
    ///
    /// The title when the page has been parsed; the file's own relative path
    /// when it has not, which is what Antora shows for a target it cannot name
    /// and is more useful to whoever has to fix it than an empty link.
    pub fn display_text(&self) -> String {
        self.title
            .clone()
            .unwrap_or_else(|| self.key.relative.clone())
    }
}

/// One version of one component: its descriptor, and the files that belong to
/// it.
#[derive(Clone, Debug)]
pub struct ComponentVersion {
    /// What `antora.yml` said.
    pub descriptor: Descriptor,

    /// Where its files came from.
    pub origin: Arc<Origin>,

    /// The navigation files it listed, as keys into the catalog, in the order
    /// their lists should appear.
    pub nav: Vec<Key>,
}

impl ComponentVersion {
    /// The version, with an unversioned component's empty string.
    pub fn version(&self) -> String {
        self.descriptor.version()
    }
}

/// One component, and every version of it.
#[derive(Clone, Debug)]
pub struct Component {
    /// The component name.
    pub name: String,

    /// Its versions, newest first.
    ///
    /// The order is the one the UI shows in the version selector, and its
    /// first entry is not necessarily [`latest`](Self::latest): a prerelease
    /// sorts to the top but is never the latest.
    pub versions: Vec<ComponentVersion>,
}

impl Component {
    /// What the UI shows for the component.
    ///
    /// Versions may disagree about the title — a component renamed in 3.0 is
    /// still the same component — and the newest one wins, because that is the
    /// name the project goes by now.
    pub fn title(&self) -> String {
        self.versions
            .first()
            .map_or_else(|| self.name.clone(), |version| version.descriptor.title())
    }

    /// The version a reference that names no version means.
    ///
    /// The greatest version that is not a prerelease, or — if every version is
    /// one — the greatest of those, because a component that has only ever
    /// shipped prereleases still has to resolve.
    pub fn latest(&self) -> Option<&ComponentVersion> {
        self.versions
            .iter()
            .find(|version| !version.descriptor.is_prerelease())
            .or_else(|| self.versions.first())
    }

    /// The greatest prerelease, if there is one.
    pub fn latest_prerelease(&self) -> Option<&ComponentVersion> {
        self.versions
            .iter()
            .find(|version| version.descriptor.is_prerelease())
    }

    /// One named version.
    pub fn version(&self, version: &str) -> Option<&ComponentVersion> {
        self.versions
            .iter()
            .find(|candidate| candidate.version() == version)
    }
}

/// Every resource in the site.
#[derive(Clone, Debug)]
pub struct Catalog {
    /// Every file, keyed by its resource ID.
    files: BTreeMap<Key, SourceFile>,

    /// Every component, keyed by name.
    components: BTreeMap<String, Component>,

    /// How a page's path becomes a URL.
    style: ExtensionStyle,
}

/// Why a file could not be added to the catalog.
///
/// A duplicate carries both contending paths and the ID they fought over, which
/// makes the type wide — so callers box it rather than widen every `Result` in
/// the build.
#[derive(Clone, Debug, thiserror::Error)]
pub enum CatalogError {
    /// Two files claim the same resource ID.
    #[error("`{key}` is claimed by both `{first}` and `{second}`")]
    Duplicate {
        /// The contested ID.
        key: Key,

        /// The file that claimed it first.
        first: String,

        /// The file that claimed it second.
        second: String,
    },
}

impl Catalog {
    /// An empty catalog that will publish pages in the given style.
    pub fn new(style: ExtensionStyle) -> Self {
        Self {
            files: BTreeMap::new(),
            components: BTreeMap::new(),
            style,
        }
    }

    /// How pages are published.
    pub fn extension_style(&self) -> ExtensionStyle {
        self.style
    }

    /// Record a component version and the navigation files it names.
    pub fn add_component_version(&mut self, component_version: ComponentVersion) {
        let name = component_version.descriptor.name.clone();

        let component = self.components.entry(name.clone()).or_insert(Component {
            name,
            versions: Vec::new(),
        });

        component.versions.push(component_version);

        // Newest first, which is the order the version selector shows and the
        // order `latest` is looked for in.
        component
            .versions
            .sort_by(|a, b| version::compare(&b.version(), &a.version()));
    }

    /// Add one file, working out where it is published as it goes.
    pub fn add(
        &mut self,
        key: Key,
        origin: Arc<Origin>,
        contents: Contents,
        relative_src_path: String,
    ) -> Result<(), Box<CatalogError>> {
        let location = locate(&key, self.style);

        match self.files.entry(key.clone()) {
            Entry::Occupied(occupied) => Err(Box::new(CatalogError::Duplicate {
                first: occupied.get().describe(),
                second: contents.describe(&key.to_string()),
                key,
            })),

            Entry::Vacant(vacant) => {
                vacant.insert(SourceFile {
                    key,
                    origin,
                    contents,
                    relative_src_path,
                    location,
                    title: None,
                    nav_title: None,
                    redirect_to: None,
                });

                Ok(())
            }
        }
    }

    /// Record what a page turned out to be called.
    pub fn set_page_titles(&mut self, key: &Key, title: Option<String>, nav_title: Option<String>) {
        if let Some(file) = self.files.get_mut(key) {
            file.title = title;
            file.nav_title = nav_title;
        }
    }

    /// Publish a redirect from `alias` to the page at `target`.
    ///
    /// An alias occupies a resource ID of its own, so a `page-aliases` entry
    /// that collides with a real page is reported rather than silently
    /// shadowing it — a redirect written over a page would make the page
    /// unreachable, and nothing else would say so.
    pub fn add_alias(&mut self, alias: Key, target: &Key) -> Result<(), Box<CatalogError>> {
        let Some(page) = self.files.get(target) else {
            return Ok(());
        };

        let origin = Arc::clone(&page.origin);
        let contents = page.contents.clone();
        let relative_src_path = page.relative_src_path.clone();
        let title = page.title.clone();
        let location = locate(&alias, self.style);

        match self.files.entry(alias.clone()) {
            Entry::Occupied(occupied) => Err(Box::new(CatalogError::Duplicate {
                first: occupied.get().describe(),
                second: contents.describe(&alias.to_string()),
                key: alias,
            })),

            Entry::Vacant(vacant) => {
                vacant.insert(SourceFile {
                    key: alias,
                    origin,
                    contents,
                    relative_src_path,
                    location,
                    title,
                    nav_title: None,
                    redirect_to: Some(target.clone()),
                });

                Ok(())
            }
        }
    }

    /// The file with this exact ID.
    pub fn get(&self, key: &Key) -> Option<&SourceFile> {
        self.files.get(key)
    }

    /// Look up a reference that may have left its version to the catalog.
    ///
    /// This is the one place "latest" is decided, so a reference to another
    /// component resolves the same way wherever it was written.
    pub fn resolve(&self, id: &ResolvedId) -> Option<&SourceFile> {
        let version = match &id.version {
            Some(version) => version.clone(),
            None => self.components.get(&id.component)?.latest()?.version(),
        };

        self.files.get(&Key {
            component: id.component.clone(),
            version,
            module: id.module.clone(),
            family: id.family,
            relative: id.relative.clone(),
        })
    }

    /// Every component, in name order.
    pub fn components(&self) -> impl Iterator<Item = &Component> {
        self.components.values()
    }

    /// One component by name.
    pub fn component(&self, name: &str) -> Option<&Component> {
        self.components.get(name)
    }

    /// Every file, in ID order.
    pub fn files(&self) -> impl Iterator<Item = &SourceFile> {
        self.files.values()
    }

    /// Every file of one family, in ID order.
    pub fn family(&self, family: Family) -> impl Iterator<Item = &SourceFile> {
        self.files
            .values()
            .filter(move |file| file.key.family == family)
    }

    /// Every page of one component version, in ID order.
    pub fn pages_of(&self, component: &str, version: &str) -> impl Iterator<Item = &SourceFile> {
        self.files.values().filter(move |file| {
            file.key.family == Family::Page
                && file.key.component == component
                && file.key.version == version
        })
    }

    /// Every version of every component that publishes the page at `relative`
    /// in `module`, for the version selector.
    ///
    /// The selector offers every version of the component, not only the ones
    /// that have this page — a version that has not got it still gets an entry,
    /// marked missing, pointing at its start page. Dropping it would make the
    /// selector's contents depend on which page the reader happens to be on.
    pub fn page_versions(&self, key: &Key) -> Vec<PageVersion<'_>> {
        let Some(component) = self.components.get(&key.component) else {
            return Vec::new();
        };

        component
            .versions
            .iter()
            .filter_map(|component_version| {
                let version = component_version.version();

                let here = Key {
                    version: version.clone(),
                    ..key.clone()
                };

                let (file, missing) = match self.files.get(&here) {
                    Some(file) => (file, false),
                    None => (self.start_page(&key.component, &version)?, true),
                };

                Some(PageVersion {
                    version,
                    display_version: component_version.descriptor.display_version(),
                    url: file.url()?.to_string(),
                    missing,
                    component_version,
                })
            })
            .collect()
    }

    /// The page a component version's own name resolves to.
    ///
    /// `start_page:` in the descriptor names it; without one it is `index.adoc`
    /// in `ROOT`, which is where a reader who typed only the component name
    /// expects to land.
    pub fn start_page(&self, component: &str, version: &str) -> Option<&SourceFile> {
        let component_version = self.components.get(component)?.version(version)?;

        let id = match &component_version.descriptor.start_page {
            Some(spec) => antors_model::ResourceId::parse(spec).ok()?.resolve_in(
                &antors_model::resource::Context {
                    component: component.to_string(),
                    version: version.to_string(),
                    module: antors_model::resource::ROOT_MODULE.to_string(),
                },
                Family::Page,
            ),

            None => ResolvedId {
                component: component.to_string(),
                version: Some(version.to_string()),
                module: antors_model::resource::ROOT_MODULE.to_string(),
                family: Family::Page,
                relative: "index.adoc".to_string(),
            },
        };

        // A `start_page:` naming another component is meaningful — it is how a
        // component points at a shared landing page — so the version is only
        // forced to this one when the reference stayed at home.
        self.resolve(&id)
    }
}

/// One entry of the version selector.
#[derive(Clone, Debug)]
pub struct PageVersion<'a> {
    /// The version, as it appears in the URL.
    pub version: String,

    /// The version, as the reader sees it.
    pub display_version: String,

    /// Where the entry links to: this page in that version, or that version's
    /// start page when it has not got this page.
    pub url: String,

    /// Whether that version has this page at all.
    pub missing: bool,

    /// The component version the entry belongs to.
    pub component_version: &'a ComponentVersion,
}
