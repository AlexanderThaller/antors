//! Building the shell's model for one page.
//!
//! Every URL in a page's shell is relative to that page, so the whole model is
//! built per page rather than shared: the navigation, the version selector and
//! the component list all say the same things, written from where the reader
//! is standing.

use antors_asciidoc::{
    Rendered,
    nav,
};
use antors_content::{
    Catalog,
    ComponentVersion,
    SourceFile,
};
use antors_model::{
    Family,
    playbook::Playbook,
    resource::Key,
    url::relativize,
};
use antors_ui::model as ui;

use crate::build::Navigation;

/// Build the shell's model for one page.
pub(crate) fn model(
    playbook: &Playbook,
    catalog: &Catalog,
    component_version: &ComponentVersion,
    navigation: &Navigation,
    page: &SourceFile,
    rendered: &Rendered,
) -> ui::Page {
    let url = page.url().unwrap_or_default().to_string();
    let root_path = page
        .location
        .as_ref()
        .map(|location| location.root_path.clone())
        .unwrap_or_default();

    let descriptor = &component_version.descriptor;

    let component = catalog.component(&descriptor.name);

    let is_latest = component
        .and_then(antors_content::Component::latest)
        .is_some_and(|latest| latest.version() == descriptor.version());

    let component_url = catalog
        .start_page(&descriptor.name, &descriptor.version())
        .and_then(SourceFile::url)
        .map_or_else(|| "./".to_string(), |start| relativize(&url, start));

    ui::Page {
        site: site(playbook, catalog, &url),
        content: rendered.html.clone(),
        title: rendered.title.clone(),
        details: details(catalog, page, &url, &rendered.details),
        description: rendered.description.clone(),
        keywords: rendered.keywords.clone(),
        canonical_url: canonical(playbook, &url),
        role: rendered.page_attributes.get("role").cloned(),

        component: ui::Component {
            name: descriptor.name.clone(),
            title: descriptor.title(),
            version: descriptor.version(),
            display_version: descriptor.display_version(),
            url: component_url,
            is_latest,
            is_versioned: component.is_some_and(|component| component.versions.len() > 1),
        },

        navigation: nav_items(&navigation.items, &url),
        breadcrumbs: breadcrumbs(&descriptor.title(), &navigation.items, &url),
        versions: versions(catalog, page, &url),
        components: components(catalog, page, &url),

        toc: rendered.toc.clone(),

        edit_url: edit_url(page, rendered),

        previous: navigation
            .previous(&url)
            .map(|link| relative_link(link, &url)),
        next: navigation.next(&url).map(|link| relative_link(link, &url)),

        root_path,
        url,
    }
}

/// The facts a page states about itself, with its tags pointing at the
/// overview.
///
/// A tag is only worth showing if it leads somewhere: the point of tagging a
/// design note `clickhouse` is to find the others. So each tag links to its
/// section of the component version's tags page — when there is one, which
/// there is not if nothing was tagged or the build was told not to make one.
fn details(
    catalog: &Catalog,
    page: &SourceFile,
    url: &str,
    details: &[antors_asciidoc::Detail],
) -> Vec<ui::Detail> {
    let overview = catalog
        .get(&Key {
            component: page.key.component.clone(),
            version: page.key.version.clone(),
            module: antors_model::resource::ROOT_MODULE.to_string(),
            family: Family::Page,
            relative: crate::tags::FILENAME.to_string(),
        })
        .and_then(SourceFile::url)
        .map(|overview| relativize(url, overview));

    details
        .iter()
        .map(|detail| ui::Detail {
            label: detail.label.clone(),
            is_list: detail.is_list,
            values: detail
                .values
                .iter()
                .map(|value| ui::DetailValue {
                    text: value.text.clone(),

                    href: match (&value.mailto, detail.name.as_str()) {
                        (Some(address), _) => Some(format!("mailto:{address}")),

                        (None, "tags") => overview.as_ref().map(|overview| {
                            format!("{overview}#{}", crate::tags::anchor(&value.text))
                        }),

                        _ => None,
                    },
                })
                .collect(),
        })
        .collect()
}

/// What the whole site is called, written from this page.
fn site(playbook: &Playbook, catalog: &Catalog, url: &str) -> ui::Site {
    let home = start_page(playbook, catalog);

    ui::Site {
        title: playbook
            .site
            .title
            .clone()
            .unwrap_or_else(|| "Documentation".to_string()),
        url: playbook.site.url.clone(),
        home_url: home.as_deref().map(|home| relativize(url, home)),
        at_home: home.as_deref() == Some(url),
    }
}

/// The page `/` redirects to, as an absolute site path.
pub(crate) fn start_page(playbook: &Playbook, catalog: &Catalog) -> Option<String> {
    if let Some(spec) = &playbook.site.start_page
        && let Some(url) = resolve_start_page(catalog, spec)
    {
        return Some(url);
    }

    // No `start_page:` and nothing at it: the first component's own start page
    // is the closest thing the site has to a front door.
    catalog
        .components()
        .next()
        .and_then(antors_content::Component::latest)
        .and_then(|version| {
            catalog.start_page(&version.descriptor.name, &version.descriptor.version())
        })
        .and_then(SourceFile::url)
        .map(str::to_string)
}

/// Resolve a `site.start_page` resource ID.
fn resolve_start_page(catalog: &Catalog, spec: &str) -> Option<String> {
    let id = antors_model::ResourceId::parse(spec).ok()?;

    // A start page names a component, so there is no page for it to be
    // relative to; the component it names is the whole of its context.
    let component = id.component.clone()?;

    let resolved = id.resolve_in(
        &antors_model::resource::Context {
            component,
            version: String::new(),
            module: antors_model::resource::ROOT_MODULE.to_string(),
        },
        Family::Page,
    );

    catalog
        .resolve(&resolved)
        .and_then(SourceFile::url)
        .map(str::to_string)
}

/// The page's canonical URL, when the site knows its own address.
fn canonical(playbook: &Playbook, url: &str) -> Option<String> {
    let base = playbook.site.url.as_ref()?.trim_end_matches('/');

    Some(format!("{base}{url}"))
}

/// Where this page can be edited.
///
/// A `page-edit-url` attribute wins outright: it is how a page that is
/// generated, or that lives somewhere other than where it was collected from,
/// says where its real source is.
fn edit_url(page: &SourceFile, rendered: &Rendered) -> Option<String> {
    if let Some(url) = rendered.page_attributes.get("edit-url") {
        return (!url.is_empty()).then(|| url.clone());
    }

    page.origin.edit_url(&page.relative_src_path)
}

/// The navigation, with every link written from this page.
fn nav_items(items: &[nav::Item], url: &str) -> Vec<ui::NavItem> {
    items
        .iter()
        .map(|item| {
            let is_current =
                item.link_type == nav::LinkType::Internal && item.url.as_deref() == Some(url);

            ui::NavItem {
                content: item.content.clone(),
                href: item.href(url),
                is_current,
                is_on_path: item.contains(url),
                items: nav_items(&item.items, url),
            }
        })
        .collect()
}

/// The trail from the component down to this page.
///
/// It follows the navigation rather than the directory structure, because the
/// navigation is where the author said how the pages relate — a page three
/// directories deep may be a top-level entry, and a page beside another may be
/// nested under it.
fn breadcrumbs(component_title: &str, items: &[nav::Item], url: &str) -> Vec<ui::Crumb> {
    let mut crumbs = vec![ui::Crumb {
        content: antors_ui::escape::text(component_title),
        href: None,
    }];

    let mut trail = Vec::new();

    if !descend(items, url, &mut trail) {
        return crumbs;
    }

    for item in trail {
        crumbs.push(ui::Crumb {
            content: item.content.clone(),
            href: item.href(url),
        });
    }

    crumbs
}

/// Walk the navigation to `url`, collecting the entries on the way.
fn descend<'a>(items: &'a [nav::Item], url: &str, trail: &mut Vec<&'a nav::Item>) -> bool {
    for item in items {
        if !item.contains(url) {
            continue;
        }

        // A file's own root entry has neither text nor link — it is the
        // navigation file, not a place — so it is walked through rather than
        // added.
        let named = !item.content.is_empty() || item.url.is_some();

        if named {
            trail.push(item);
        }

        if item.url.as_deref() == Some(url) {
            return true;
        }

        if descend(&item.items, url, trail) {
            return true;
        }

        if named {
            trail.pop();
        }
    }

    false
}

/// The version selector, written from this page.
fn versions(catalog: &Catalog, page: &SourceFile, url: &str) -> Vec<ui::VersionEntry> {
    catalog
        .page_versions(&page.key)
        .into_iter()
        .map(|version| ui::VersionEntry {
            display_version: version.display_version,
            href: relativize(url, &version.url),
            is_current: version.version == page.key.version,
            is_missing: version.missing,
        })
        .collect()
}

/// Every component in the site, written from this page.
fn components(catalog: &Catalog, page: &SourceFile, url: &str) -> Vec<ui::ComponentEntry> {
    catalog
        .components()
        .filter_map(|component| {
            let latest = component.latest()?;
            let home = catalog
                .start_page(&component.name, &latest.version())
                .and_then(SourceFile::url)?;

            let versions = if component.versions.len() > 1 {
                component
                    .versions
                    .iter()
                    .filter_map(|version| {
                        let start = catalog
                            .start_page(&component.name, &version.version())
                            .and_then(SourceFile::url)?;

                        Some(ui::ComponentVersionEntry {
                            display_version: version.descriptor.display_version(),
                            href: relativize(url, start),
                            is_current: component.name == page.key.component
                                && version.version() == page.key.version,
                            is_latest: version.version() == latest.version(),
                        })
                    })
                    .collect()
            } else {
                Vec::new()
            };

            Some(ui::ComponentEntry {
                title: component.title(),
                href: relativize(url, home),
                is_current: component.name == page.key.component,
                versions,
            })
        })
        .collect()
}

/// A navigation link, written from this page.
fn relative_link(item: &nav::Item, url: &str) -> ui::Link {
    ui::Link {
        content: item.content.clone(),
        href: item.href(url).unwrap_or_default(),
    }
}
