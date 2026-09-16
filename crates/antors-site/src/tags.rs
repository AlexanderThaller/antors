//! The tags overview.
//!
//! A page may tag itself — `:page-tags: asciidoc, rendering` — and a set of
//! design notes tagged that way is only useful if something gathers them. That
//! something is an ordinary page: this generates its `AsciiDoc` and puts it in
//! the catalog *before* anything renders.
//!
//! Generating source rather than markup is what makes it ordinary. A reference
//! to `xref:tags.adoc[]` resolves because the catalog has it, a navigation file
//! may list it, it gets a title, an outline, breadcrumbs, a version selector
//! and a place in the pagination — none of which would be true of a page
//! written straight into the output directory at the end.

use std::{
    collections::BTreeMap,
    sync::Arc,
};

use antors_content::{
    Catalog,
    Contents,
};
use antors_model::{
    Family,
    resource::{
        Key,
        ROOT_MODULE,
    },
};

use crate::report::Report;

/// Where the overview is published, within its component version's `ROOT`.
pub(crate) const FILENAME: &str = "tags.adoc";

/// What one page contributes to the overview.
#[derive(Clone, Debug)]
pub(crate) struct Tagged {
    /// The page.
    pub key: Key,

    /// What it calls itself.
    pub title: Option<String>,

    /// What it tagged itself with.
    pub tags: Vec<String>,
}

/// Add a tags overview to every component version that has any tagged pages.
///
/// A component version whose pages carry no tags gets no page: an empty
/// overview is a link to nothing, and one in the navigation is worse.
pub(crate) fn generate(catalog: &mut Catalog, tagged: &[Tagged], report: &mut Report) {
    let mut by_version: BTreeMap<(String, String), Vec<&Tagged>> = BTreeMap::new();

    for page in tagged {
        if page.tags.is_empty() {
            continue;
        }

        by_version
            .entry((page.key.component.clone(), page.key.version.clone()))
            .or_default()
            .push(page);
    }

    for ((component, version), pages) in by_version {
        let key = Key {
            component: component.clone(),
            version: version.clone(),
            module: ROOT_MODULE.to_string(),
            family: Family::Page,
            relative: FILENAME.to_string(),
        };

        // A component that already has a page there wrote one on purpose — an
        // introduction, a note about what the tags mean — so the list is
        // *added* to it rather than put in its place. A page that says nothing
        // but `= Tags` is the common case, and appending is what makes it work
        // without being emptied first.
        if let Some(existing) = catalog.get(&key) {
            let Ok(preamble) = existing.contents.read_to_string() else {
                report.warn(
                    FILENAME,
                    None,
                    "the tags page could not be read, so its list was not added",
                );

                continue;
            };

            let source = format!("{}\n\n{}", preamble.trim_end(), sections(&pages));
            catalog.set_contents(&key, Contents::from(source.into_bytes()));

            continue;
        }

        let Some(origin) = catalog
            .component(&component)
            .and_then(|component| component.version(&version))
            .map(|component_version| Arc::clone(&component_version.origin))
        else {
            continue;
        };

        let source = format!(
            "= Tags\n:description: Every tagged page, by tag.\n\n{}",
            sections(&pages)
        );

        if let Err(error) = catalog.add(
            key.clone(),
            origin,
            Contents::from(source.into_bytes()),
            FILENAME.to_string(),
        ) {
            report.warn(FILENAME, None, error.to_string());
            continue;
        }

        catalog.set_page_titles(&key, Some("Tags".to_string()), None);
    }
}

/// The overview's body: everything below its title.
fn sections(pages: &[&Tagged]) -> String {
    let grouped = group(pages);

    let mut out = String::new();

    let _ = std::fmt::Write::write_fmt(
        &mut out,
        format_args!(
            "{} tag{} across {} page{}.\n\n",
            grouped.len(),
            plural(grouped.len()),
            pages.len(),
            plural(pages.len()),
        ),
    );

    for (slug, group) in grouped {
        let mut tagged = group.pages;
        tagged.sort_by_key(|page| title_of(page));

        let _ = std::fmt::Write::write_fmt(
            &mut out,
            format_args!("[#{ANCHOR_PREFIX}{slug}]\n== {}\n\n", group.label),
        );

        for page in tagged {
            // Each entry is a real cross-reference, so it is resolved and
            // checked like every other one — a tagged page that is deleted
            // turns into a reported broken reference rather than a dead link.
            let _ = std::fmt::Write::write_fmt(
                &mut out,
                format_args!("* xref:{}[]\n", reference(&page.key)),
            );
        }

        out.push('\n');
    }

    out
}

/// One tag, under whichever spelling of it is most used.
struct Group<'a> {
    /// What the section is titled.
    label: String,

    /// The pages carrying it, each once.
    pages: Vec<&'a Tagged>,
}

/// Gather pages by tag, merging the spellings of one tag into one.
///
/// `Session Store`, `session-store` and `session store` are one tag written
/// three ways, and an index that listed them as three would be worse than no
/// index — each entry showing a third of the pages, and none of them saying so.
/// So tags are grouped by what they reduce to, and the section is titled with
/// whichever spelling is most used.
fn group<'a>(pages: &[&'a Tagged]) -> BTreeMap<String, Group<'a>> {
    let mut spellings: BTreeMap<String, BTreeMap<&str, usize>> = BTreeMap::new();
    let mut members: BTreeMap<String, Vec<&'a Tagged>> = BTreeMap::new();

    for page in pages {
        // A page that wrote two spellings of one tag is listed under it once.
        let mut seen: Vec<String> = Vec::new();

        for tag in &page.tags {
            let slug = slug(tag);

            if slug.is_empty() {
                continue;
            }

            *spellings
                .entry(slug.clone())
                .or_default()
                .entry(tag.as_str())
                .or_default() += 1;

            if seen.contains(&slug) {
                continue;
            }

            seen.push(slug.clone());
            members.entry(slug).or_default().push(page);
        }
    }

    members
        .into_iter()
        .map(|(slug, pages)| {
            let label = spellings
                .get(&slug)
                .and_then(|spellings| {
                    spellings
                        .iter()
                        // Most used wins; between equals the one that sorts
                        // first, so the answer does not depend on page order.
                        .max_by_key(|(spelling, count)| (**count, std::cmp::Reverse(*spelling)))
                        .map(|(spelling, _)| (*spelling).to_string())
                })
                .unwrap_or_else(|| slug.clone());

            (slug, Group { label, pages })
        })
        .collect()
}

/// A page's title, for ordering.
fn title_of(page: &Tagged) -> String {
    page.title
        .clone()
        .unwrap_or_else(|| page.key.relative.clone())
}

/// The resource ID the overview refers to a page by.
///
/// The overview sits in `ROOT`, so a page in another module needs naming; one
/// in `ROOT` does not.
fn reference(key: &Key) -> String {
    if key.module == ROOT_MODULE {
        key.relative.clone()
    } else {
        format!("{}:{}", key.module, key.relative)
    }
}

/// What every tag anchor starts with, so the ids a tags page adds cannot
/// collide with an author's own.
const ANCHOR_PREFIX: &str = "tag-";

/// The anchor a tag's section carries, so a page's own tag can link straight to
/// it.
pub(crate) fn anchor(tag: &str) -> String {
    format!("{ANCHOR_PREFIX}{}", slug(tag))
}

/// What a tag reduces to.
///
/// A tag may be anything an author typed, and an id may not — so everything
/// that is not a letter or a digit becomes a dash, and the result is folded to
/// lower case. That also decides which tags are *the same* tag: `Session Store`
/// and `session-store` both reduce to `session-store`, and so end up in one
/// section.
///
/// A tag with nothing to reduce — punctuation, an emoji — reduces to the empty
/// string, which is how the caller knows there is nothing to index it under.
fn slug(tag: &str) -> String {
    let mut slug = String::new();
    let mut last_was_dash = true;

    for character in tag.chars() {
        if character.is_ascii_alphanumeric() {
            slug.extend(character.to_lowercase());
            last_was_dash = false;
        } else if !last_was_dash {
            slug.push('-');
            last_was_dash = true;
        }
    }

    slug.trim_end_matches('-').to_string()
}

/// An `s`, when there should be one.
fn plural(count: usize) -> &'static str {
    if count == 1 { "" } else { "s" }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tagged(module: &str, relative: &str, title: &str, tags: &[&str]) -> Tagged {
        Tagged {
            key: Key {
                component: "c".to_string(),
                version: "1.0".to_string(),
                module: module.to_string(),
                family: Family::Page,
                relative: relative.to_string(),
            },
            title: Some(title.to_string()),
            tags: tags.iter().map(|tag| (*tag).to_string()).collect(),
        }
    }

    #[test]
    fn an_anchor_survives_anything_an_author_typed() {
        assert_eq!(anchor("asciidoc"), "tag-asciidoc");
        assert_eq!(anchor("Living Document"), "tag-living-document");
        assert_eq!(anchor("c++"), "tag-c");
        assert_eq!(anchor("a/b"), "tag-a-b");
    }

    #[test]
    fn spellings_of_one_tag_reduce_to_one_slug() {
        for spelling in [
            "session-store",
            "Session Store",
            "session store",
            "SESSION_STORE",
        ] {
            assert_eq!(slug(spelling), "session-store", "{spelling}");
        }
    }

    #[test]
    fn a_tag_with_nothing_in_it_reduces_to_nothing() {
        assert_eq!(slug("!!!"), "");
        assert_eq!(slug("  "), "");
    }

    #[test]
    fn a_page_outside_root_is_named_by_its_module() {
        let root = tagged("ROOT", "a.adoc", "A", &[]);
        let other = tagged("design", "b.adoc", "B", &[]);

        assert_eq!(reference(&root.key), "a.adoc");
        assert_eq!(reference(&other.key), "design:b.adoc");
    }

    #[test]
    fn spellings_of_one_tag_become_one_section() {
        let pages = [
            tagged("ROOT", "a.adoc", "Alpha", &["session-store"]),
            tagged("ROOT", "b.adoc", "Beta", &["Session Store"]),
            tagged("ROOT", "c.adoc", "Gamma", &["session store"]),
            tagged("ROOT", "d.adoc", "Delta", &["session-store"]),
        ];

        let source = sections(&pages.iter().collect::<Vec<&Tagged>>());

        assert!(source.starts_with("1 tag across 4 pages.\n"), "{source}");
        assert_eq!(source.matches("[#tag-session-store]").count(), 1);

        // The most-used spelling titles the section.
        assert!(source.contains("== session-store\n"), "{source}");

        for page in ["a.adoc", "b.adoc", "c.adoc", "d.adoc"] {
            assert!(source.contains(page), "{page} is missing from {source}");
        }
    }

    #[test]
    fn a_page_that_wrote_a_tag_twice_is_listed_once() {
        let pages = [tagged("ROOT", "a.adoc", "Alpha", &["api", "API"])];
        let source = sections(&pages.iter().collect::<Vec<&Tagged>>());

        assert_eq!(source.matches("a.adoc").count(), 1, "{source}");
    }

    #[test]
    fn a_tag_that_reduces_to_nothing_is_dropped() {
        let pages = [tagged("ROOT", "a.adoc", "Alpha", &["!!!", "real"])];
        let source = sections(&pages.iter().collect::<Vec<&Tagged>>());

        assert!(source.starts_with("1 tag across 1 page.\n"), "{source}");
    }

    #[test]
    fn the_overview_groups_pages_under_each_tag() {
        let pages = [
            tagged("ROOT", "a.adoc", "Alpha", &["one", "two"]),
            tagged("design", "b.adoc", "Beta", &["two"]),
        ];

        let source = sections(&pages.iter().collect::<Vec<&Tagged>>());

        assert!(source.starts_with("2 tags across 2 pages.\n"));
        assert!(source.contains("[#tag-one]\n== one\n"));
        assert!(source.contains("[#tag-two]\n== two\n"));
        assert!(source.contains("* xref:a.adoc[]\n"));
        assert!(source.contains("* xref:design:b.adoc[]\n"));
    }

    #[test]
    fn pages_under_a_tag_are_in_title_order() {
        let pages = [
            tagged("ROOT", "z.adoc", "Zulu", &["t"]),
            tagged("ROOT", "a.adoc", "Alpha", &["t"]),
        ];

        let source = sections(&pages.iter().collect::<Vec<&Tagged>>());
        let alpha = source.find("a.adoc").expect("Alpha is listed");
        let zulu = source.find("z.adoc").expect("Zulu is listed");

        assert!(alpha < zulu, "{source}");
    }
}
