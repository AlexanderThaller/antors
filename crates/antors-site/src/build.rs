//! Running a build from end to end.

use std::{
    collections::BTreeMap,
    path::PathBuf,
    sync::Arc,
};

use antors_asciidoc::{
    Renderer,
    Warning,
    nav,
};
use antors_content::{
    Catalog,
    ComponentVersion,
    SourceFile,
    aggregate::{
        AggregateError,
        aggregate,
    },
};
use antors_model::{
    Family,
    Playbook,
    ResourceId,
    resource::{
        Context,
        Key,
    },
    url::locate,
};

use crate::{
    page,
    report::{
        Report,
        Severity,
    },
    sitemap,
    write::Writer,
};

/// What the renderer should do with the parts of a page it can leave out.
///
/// Re-exported from [`antors-asciidoc`] so a caller that only builds a site
/// need not depend on the rendering crate to configure one.
///
/// [`antors-asciidoc`]: https://crates.io/crates/antors-asciidoc
pub type RenderOptions = antors_asciidoc::Options;

/// What to leave out of a build.
#[derive(Clone, Copy, Debug, Default)]
pub struct Options {
    /// What the renderer should do with the parts of a page it can leave out.
    pub render: antors_asciidoc::Options,

    /// Whether the output directory is emptied first.
    ///
    /// The playbook's `output.clean` says this too; this is the command line's
    /// way to say it for one run.
    pub clean: bool,
}

/// Why a build could not run.
///
/// These are the failures that stop a build rather than appear in its report:
/// a source that cannot be read, an output directory that cannot be written.
/// Everything a page gets wrong is a [`Problem`](crate::Problem) instead.
#[derive(Debug, thiserror::Error)]
pub enum BuildError {
    /// The content could not be collected.
    #[error(transparent)]
    Aggregate(#[from] AggregateError),

    /// The site could not be written.
    #[error("writing `{}`", path.display())]
    Write {
        /// The path that could not be written.
        path: PathBuf,

        /// What the file system said.
        #[source]
        error: std::io::Error,
    },
}

/// One component version's navigation, and the page order that follows from
/// it.
#[derive(Clone, Debug, Default)]
pub struct Navigation {
    /// The tree the sidebar draws.
    pub items: Vec<nav::Item>,

    /// Every page the navigation links to, in the order it links to them.
    ///
    /// This is what "previous" and "next" mean: the order an author put the
    /// pages in, not the order the file system happens to list them.
    order: Vec<nav::Item>,
}

impl Navigation {
    /// Build the navigation for one component version.
    fn build(catalog: &Arc<Catalog>, component_version: &ComponentVersion) -> Self {
        let items = nav::build(catalog, component_version);
        let mut order = Vec::new();

        flatten(&items, &mut order);

        Self { items, order }
    }

    /// The page before the one at `url`.
    pub fn previous(&self, url: &str) -> Option<&nav::Item> {
        let index = self.index_of(url)?;

        index
            .checked_sub(1)
            .and_then(|before| self.order.get(before))
    }

    /// The page after the one at `url`.
    pub fn next(&self, url: &str) -> Option<&nav::Item> {
        self.order.get(self.index_of(url)? + 1)
    }

    /// Where `url` sits in the navigation order.
    fn index_of(&self, url: &str) -> Option<usize> {
        self.order
            .iter()
            .position(|item| item.url.as_deref() == Some(url))
    }
}

/// Collect every page the navigation links to, depth first.
fn flatten(items: &[nav::Item], order: &mut Vec<nav::Item>) {
    for item in items {
        if item.link_type == nav::LinkType::Internal && item.url.is_some() {
            order.push(nav::Item {
                items: Vec::new(),
                ..item.clone()
            });
        }

        flatten(&item.items, order);
    }
}

/// One build.
#[derive(Debug)]
pub struct Build {
    /// What to build.
    playbook: Playbook,

    /// What to leave out.
    options: Options,
}

impl Build {
    /// Prepare a build.
    pub fn new(playbook: Playbook, options: Options) -> Self {
        Self { playbook, options }
    }

    /// Run it.
    pub fn run(&self) -> Result<Report, BuildError> {
        let mut report = Report::default();

        let catalog = aggregate(&self.playbook)?;

        // The titles pass. Every page's name has to be known before any page's
        // references are resolved, because a reference with no text of its own
        // shows the name of the page it points at.
        let catalog = self.name_pages(catalog, &mut report);
        let catalog = Arc::new(catalog);

        let navigation = Self::navigation(&catalog);

        let mut writer = Writer::new(self.playbook.output.dir.clone());

        if self.options.clean || self.playbook.output.clean {
            writer.clean()?;
        }

        self.write_pages(&catalog, &navigation, &mut writer, &mut report)?;
        Self::write_resources(&catalog, &mut writer, &mut report);
        self.write_redirects(&catalog, &mut writer, &mut report)?;
        self.write_site_files(&catalog, &mut writer, &mut report)?;

        report.pages = writer.pages;
        report.files = writer.files;

        Ok(report)
    }

    /// Learn what every page is called, and record the IDs they also answer
    /// to.
    fn name_pages(&self, catalog: Catalog, report: &mut Report) -> Catalog {
        let shared = Arc::new(catalog.clone());
        let renderer = self.renderer(&shared);

        let mut named: Vec<(Key, antors_asciidoc::Header)> = Vec::new();

        for (component_version, page) in pages(&shared) {
            named.push((
                page.key.clone(),
                renderer.header_of(page, component_version),
            ));
        }

        drop(renderer);
        drop(shared);

        let mut catalog = catalog;

        for (key, header) in named {
            catalog.set_page_titles(&key, header.title, header.nav_title);

            for alias in header.aliases {
                Self::add_alias(&mut catalog, &key, &alias, report);
            }
        }

        catalog
    }

    /// Record one `page-aliases` entry.
    fn add_alias(catalog: &mut Catalog, page: &Key, alias: &str, report: &mut Report) {
        let file = catalog.get(page).map(|file| file.relative_src_path.clone());
        let file = file.unwrap_or_else(|| page.relative.clone());

        let Ok(id) = ResourceId::parse(alias) else {
            report.warn(file, None, format!("`{alias}` is not a resource ID"));
            return;
        };

        let resolved = id.resolve_in(
            &Context {
                component: page.component.clone(),
                version: page.version.clone(),
                module: page.module.clone(),
            },
            Family::Page,
        );

        let key = Key {
            component: resolved.component,
            version: resolved.version.unwrap_or_else(|| page.version.clone()),
            module: resolved.module,

            // An alias is not a page: it is published as a redirect, and giving
            // it a family of its own is what keeps it from colliding with a
            // real page at the same ID by accident.
            family: Family::Alias,
            relative: resolved.relative,
        };

        if let Err(error) = catalog.add_alias(key, page) {
            report.warn(file, None, error.to_string());
        }
    }

    /// Read every component version's navigation.
    fn navigation(catalog: &Arc<Catalog>) -> BTreeMap<(String, String), Navigation> {
        let mut navigation = BTreeMap::new();

        for component in catalog.components() {
            for component_version in &component.versions {
                navigation.insert(
                    (component.name.clone(), component_version.version()),
                    Navigation::build(catalog, component_version),
                );
            }
        }

        navigation
    }

    /// Render and write every page.
    fn write_pages(
        &self,
        catalog: &Arc<Catalog>,
        navigation: &BTreeMap<(String, String), Navigation>,
        writer: &mut Writer,
        report: &mut Report,
    ) -> Result<(), BuildError> {
        let renderer = self.renderer(catalog);
        let empty = Navigation::default();

        for (component_version, page) in pages(catalog) {
            let Some(location) = &page.location else {
                continue;
            };

            let article = match renderer.render(page, component_version) {
                Ok(article) => article,

                Err(error) => {
                    report.error(&page.relative_src_path, error.to_string());
                    continue;
                }
            };

            for warning in &article.warnings {
                let line = match warning {
                    Warning::Parse { line, .. } => Some(*line),
                    _ => None,
                };

                report.warn(&page.relative_src_path, line, warning.to_string());
            }

            let key = (page.key.component.clone(), page.key.version.clone());
            let navigation = navigation.get(&key).unwrap_or(&empty);

            let model = page::model(
                &self.playbook,
                catalog,
                component_version,
                navigation,
                page,
                &article,
            );

            writer.page(&location.out, &antors_ui::render(&model))?;
        }

        Ok(())
    }

    /// Copy every image and attachment.
    ///
    /// A file that cannot be copied is reported and the build goes on: one
    /// missing screenshot is not a reason to publish nothing.
    fn write_resources(catalog: &Arc<Catalog>, writer: &mut Writer, report: &mut Report) {
        for family in [Family::Image, Family::Attachment] {
            for file in catalog.family(family) {
                let Some(location) = &file.location else {
                    continue;
                };

                if let Err(error) = writer.copy(&location.out, &file.path) {
                    report.error(&file.relative_src_path, error.to_string());
                }
            }
        }
    }

    /// Write a redirect for every alias.
    fn write_redirects(
        &self,
        catalog: &Arc<Catalog>,
        writer: &mut Writer,
        report: &mut Report,
    ) -> Result<(), BuildError> {
        for alias in catalog.family(Family::Alias) {
            let Some(location) = &alias.location else {
                continue;
            };

            let Some(url) = alias
                .redirect_to
                .as_ref()
                .and_then(|target| catalog.get(target))
                .and_then(SourceFile::url)
            else {
                report.warn(
                    &alias.relative_src_path,
                    None,
                    format!("`{}` redirects to nothing", alias.key),
                );

                continue;
            };

            let href = antors_model::url::relativize(&location.url, url);
            let canonical = self
                .playbook
                .site
                .url
                .as_ref()
                .map(|base| format!("{}{url}", base.trim_end_matches('/')));

            writer.file(
                &location.out,
                antors_ui::redirect(&href, canonical.as_deref()).as_bytes(),
            )?;
        }

        Ok(())
    }

    /// The site's own files: the start-page redirect, the 404, robots and the
    /// sitemaps, plus the UI.
    fn write_site_files(
        &self,
        catalog: &Arc<Catalog>,
        writer: &mut Writer,
        _report: &mut Report,
    ) -> Result<(), BuildError> {
        for asset in antors_ui::assets::ASSETS {
            writer.file(&format!("_/{}", asset.path), asset.contents)?;
        }

        let title = self
            .playbook
            .site
            .title
            .clone()
            .unwrap_or_else(|| "Documentation".to_string());

        if let Some(start) = page::start_page(&self.playbook, catalog) {
            let canonical = self
                .playbook
                .site
                .url
                .as_ref()
                .map(|base| format!("{}{start}", base.trim_end_matches('/')));

            writer.file(
                "index.html",
                antors_ui::redirect(start.trim_start_matches('/'), canonical.as_deref()).as_bytes(),
            )?;

            // The 404 is served for a URL that does not exist, so it cannot
            // know how deep it is and has to link from the site root — which
            // only a site that knows its own base path can do.
            let base = base_path(self.playbook.site.url.as_deref());

            writer.file(
                "404.html",
                antors_ui::not_found(
                    &title,
                    &format!("{base}{}", start.trim_start_matches('/')),
                    &base,
                )
                .as_bytes(),
            )?;
        }

        if let Some(robots) = robots(&self.playbook) {
            writer.file("robots.txt", robots.as_bytes())?;
        }

        for (path, contents) in sitemap::sitemaps(&self.playbook, catalog) {
            writer.file(&path, contents.as_bytes())?;
        }

        Ok(())
    }

    /// A renderer for this build.
    fn renderer(&self, catalog: &Arc<Catalog>) -> Renderer {
        Renderer::new(
            Arc::clone(catalog),
            self.playbook.clone(),
            self.options.render,
        )
    }

    /// The severity at which this build should be called a failure.
    pub fn failure_level(&self) -> Severity {
        match self.playbook.runtime.log.failure_level.as_deref() {
            Some("warn") => Severity::Warning,
            _ => Severity::Error,
        }
    }
}

/// Every page in the site, with the component version it belongs to.
fn pages(catalog: &Catalog) -> Vec<(&ComponentVersion, &SourceFile)> {
    let mut pages = Vec::new();

    for component in catalog.components() {
        for component_version in &component.versions {
            for page in catalog.pages_of(&component.name, &component_version.version()) {
                pages.push((component_version, page));
            }
        }
    }

    pages
}

/// The site's `robots.txt`, if the playbook asked for one.
fn robots(playbook: &Playbook) -> Option<String> {
    let robots = playbook.site.robots.as_deref()?;
    let base = base_path(playbook.site.url.as_deref());

    Some(match robots {
        "allow" => match playbook.site.url.as_deref() {
            Some(_) => format!("User-agent: *\nAllow: {base}\n"),
            None => "User-agent: *\nAllow: /\n".to_string(),
        },

        "disallow" => format!("User-agent: *\nDisallow: {base}\n"),

        // Anything else is the file's contents, which is how a site says
        // something `allow` and `disallow` cannot.
        other => other.to_string(),
    })
}

/// The path part of the site's URL, with a trailing slash — `/` when the site
/// does not know its own address.
fn base_path(url: Option<&str>) -> String {
    let Some(url) = url else {
        return "/".to_string();
    };

    let path = match url.split_once("://") {
        Some((_, rest)) => rest.find('/').map_or("", |index| &rest[index..]),
        None => url,
    };

    let path = path.trim_end_matches('/');

    if path.is_empty() {
        "/".to_string()
    } else {
        format!("{path}/")
    }
}

/// Where a key would be published, for a caller that has no file to ask.
pub fn url_of(key: &Key, catalog: &Catalog) -> Option<String> {
    locate(key, catalog.extension_style()).map(|location| location.url)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_base_path_is_where_the_site_is_mounted() {
        assert_eq!(base_path(None), "/");
        assert_eq!(base_path(Some("https://example.org")), "/");
        assert_eq!(base_path(Some("https://example.org/")), "/");
        assert_eq!(base_path(Some("https://example.org/docs")), "/docs/");
        assert_eq!(base_path(Some("https://example.org/docs/")), "/docs/");
    }

    #[test]
    fn robots_says_what_the_playbook_asked_for() {
        let mut playbook = Playbook::default();

        assert_eq!(robots(&playbook), None);

        playbook.site.robots = Some("allow".to_string());
        playbook.site.url = Some("https://example.org/docs".to_string());

        assert_eq!(
            robots(&playbook).as_deref(),
            Some("User-agent: *\nAllow: /docs/\n")
        );

        playbook.site.robots = Some("User-agent: *\nDisallow: /private\n".to_string());

        assert_eq!(
            robots(&playbook).as_deref(),
            Some("User-agent: *\nDisallow: /private\n")
        );
    }
}
