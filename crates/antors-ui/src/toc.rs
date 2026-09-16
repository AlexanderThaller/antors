//! The outline beside the article.
//!
//! Antora's own UI builds this in the browser, by walking the headings after
//! the page has loaded. It is built here instead, from the section tree the
//! parse already produced — so it is in the HTML that is served, which means it
//! works without scripting, is there for a reader who prints the page, and
//! costs the browser nothing.

use std::fmt::Write as _;

use crate::{
    escape::attr,
    model::{
        Page,
        TocEntry,
    },
};

/// The outline, or nothing when the page has no headings to list.
pub(crate) fn sidebar(page: &Page) -> String {
    if page.toc.is_empty() {
        return String::new();
    }

    let mut out = format!(
        "<aside class=\"toc sidebar\" data-title=\"{}\" data-levels=\"{}\">\n<div \
         class=\"toc-menu\">\n<h3>{}</h3>\n",
        attr(&page.toc_title),
        page.toc_levels,
        crate::escape::text(&page.toc_title),
    );

    out.push_str(&list(&page.toc));
    out.push_str("</div>\n</aside>\n");

    out
}

/// One level of the outline.
fn list(entries: &[TocEntry]) -> String {
    if entries.is_empty() {
        return String::new();
    }

    let mut out = String::from("<ul>\n");

    for entry in entries {
        let _ = write!(
            out,
            "<li data-level=\"{}\"><a href=\"#{}\">{}</a>",
            entry.level,
            attr(&entry.id),
            entry.title,
        );

        out.push_str(&list(&entry.children));
        out.push_str("</li>\n");
    }

    out.push_str("</ul>\n");

    out
}
