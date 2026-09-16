//! What the shell is told about a page.
//!
//! This is the whole of the shell's input, and it is deliberately flat: every
//! URL is already relative to the page it appears on, every title is already
//! the one to show, and nothing here needs looking up. Building it is the site
//! generator's job, so that the question "where does this link go" is answered
//! once, by code that can see the catalog, rather than in the middle of
//! emitting HTML.

/// Everything the shell needs to draw one page.
#[derive(Clone, Debug, Default)]
pub struct Page {
    /// What the whole site is called and where its fixed points are.
    pub site: Site,

    /// The article's markup, without its title.
    pub content: String,

    /// The page title, shown as the `h1` and in the browser's tab.
    pub title: Option<String>,

    /// `:description:`, for the page's `<meta>` and for a search result.
    pub description: Option<String>,

    /// `:keywords:`, likewise.
    pub keywords: Option<String>,

    /// The page's canonical URL, when the site knows its own address.
    pub canonical_url: Option<String>,

    /// Extra classes for the `article` element, from `page-role`.
    pub role: Option<String>,

    /// Which component version the reader is in.
    pub component: Component,

    /// The navigation for that component version.
    pub navigation: Vec<NavItem>,

    /// The trail from the component down to this page.
    pub breadcrumbs: Vec<Crumb>,

    /// This page in every version of its component.
    pub versions: Vec<VersionEntry>,

    /// Every component in the site, for the explore panel.
    pub components: Vec<ComponentEntry>,

    /// The page's own outline.
    pub toc: Vec<TocEntry>,

    /// How deep that outline goes, for the shell to report to the script.
    pub toc_levels: usize,

    /// What the outline is titled.
    pub toc_title: String,

    /// Where this page can be edited, if anywhere.
    pub edit_url: Option<String>,

    /// The previous and next pages in navigation order.
    pub previous: Option<Link>,

    /// The next page in navigation order.
    pub next: Option<Link>,

    /// The path from this page to the site root, for the assets.
    pub root_path: String,

    /// Where this page is published, as an absolute site path.
    pub url: String,
}

/// What the whole site is called.
#[derive(Clone, Debug, Default)]
pub struct Site {
    /// The name in the navbar.
    pub title: String,

    /// The site's base URL, if it has one.
    pub url: Option<String>,

    /// The link the navbar brand and the home button point at, relative to
    /// this page.
    pub home_url: Option<String>,

    /// Whether the page being drawn *is* the home page.
    pub at_home: bool,
}

/// The component version the reader is in.
#[derive(Clone, Debug, Default)]
pub struct Component {
    /// The component's name, as it appears in a resource ID.
    pub name: String,

    /// What the UI shows for it.
    pub title: String,

    /// The version, as it appears in the URL.
    pub version: String,

    /// The version, as the reader sees it.
    pub display_version: String,

    /// The component version's start page, relative to this page.
    pub url: String,

    /// Whether this is the version a bare reference to the component means.
    pub is_latest: bool,

    /// Whether the component has more than one version.
    pub is_versioned: bool,
}

/// One entry of the navigation tree.
#[derive(Clone, Debug, Default)]
pub struct NavItem {
    /// The entry's text, already rendered.
    pub content: String,

    /// Where it points, relative to the page it is drawn on.
    pub href: Option<String>,

    /// Whether the reader is on this page.
    pub is_current: bool,

    /// Whether the page the reader is on is at or beneath this entry, which is
    /// what decides whether the list under it starts open.
    pub is_on_path: bool,

    /// The entries beneath it.
    pub items: Vec<NavItem>,
}

/// One step of the breadcrumb trail.
#[derive(Clone, Debug)]
pub struct Crumb {
    /// What it says.
    pub content: String,

    /// Where it goes, or `None` for a step that names a group rather than a
    /// page.
    pub href: Option<String>,
}

/// One entry of the version selector.
#[derive(Clone, Debug)]
pub struct VersionEntry {
    /// The version, as the reader sees it.
    pub display_version: String,

    /// Where it goes, relative to this page.
    pub href: String,

    /// Whether it is the version being read.
    pub is_current: bool,

    /// Whether that version has not got this page, and the link therefore
    /// leads to its start page instead.
    pub is_missing: bool,
}

/// One component in the explore panel.
#[derive(Clone, Debug)]
pub struct ComponentEntry {
    /// What the UI shows for it.
    pub title: String,

    /// Where its latest version starts, relative to this page.
    pub href: String,

    /// Whether it is the component being read.
    pub is_current: bool,

    /// Its versions, newest first. Empty for an unversioned component.
    pub versions: Vec<ComponentVersionEntry>,
}

/// One version of a component in the explore panel.
#[derive(Clone, Debug)]
pub struct ComponentVersionEntry {
    /// The version, as the reader sees it.
    pub display_version: String,

    /// Where it starts, relative to this page.
    pub href: String,

    /// Whether it is the version being read.
    pub is_current: bool,

    /// Whether it is the version a bare reference to the component means.
    pub is_latest: bool,
}

/// One entry of a page's outline.
#[derive(Clone, Debug)]
pub struct TocEntry {
    /// The anchor to link to.
    pub id: String,

    /// The heading, already rendered.
    pub title: String,

    /// How deep it sits.
    pub level: usize,

    /// The headings beneath it.
    pub children: Vec<TocEntry>,
}

/// A link with text.
#[derive(Clone, Debug)]
pub struct Link {
    /// What it says.
    pub content: String,

    /// Where it goes, relative to the page it is drawn on.
    pub href: String,
}
