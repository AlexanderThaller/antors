//! The article and the toolbar above it.

use std::fmt::Write as _;

use crate::{
    escape::{
        attr,
        detag,
        text,
    },
    model::Page,
};

/// The bar above the article: where the reader is, and what they can do here.
pub(crate) fn toolbar(page: &Page) -> String {
    let mut out = String::from("<div class=\"toolbar\" role=\"navigation\">\n");

    out.push_str(
        "<button class=\"nav-toggle\" aria-label=\"Show or hide the navigation\"></button>\n",
    );

    if let Some(home) = &page.site.home_url {
        let current = if page.site.at_home { " is-current" } else { "" };

        let _ = writeln!(
            out,
            "<a href=\"{}\" class=\"home-link{current}\" aria-label=\"Home\"></a>",
            attr(home),
        );
    }

    out.push_str(&breadcrumbs(page));
    out.push_str(&versions(page));
    out.push_str(&downloads(page));
    out.push_str(&edit_link(page));

    out.push_str("</div>\n");

    out
}

/// The trail from the component down to this page.
fn breadcrumbs(page: &Page) -> String {
    if page.breadcrumbs.is_empty() {
        return String::new();
    }

    let mut out = String::from("<nav class=\"breadcrumbs\" aria-label=\"breadcrumbs\">\n<ul>\n");

    for crumb in &page.breadcrumbs {
        match &crumb.href {
            Some(href) => {
                let _ = writeln!(
                    out,
                    "<li><a href=\"{}\">{}</a></li>",
                    attr(href),
                    crumb.content,
                );
            }

            None => {
                let _ = writeln!(out, "<li>{}</li>", crumb.content);
            }
        }
    }

    out.push_str("</ul>\n</nav>\n");

    out
}

/// The version selector.
fn versions(page: &Page) -> String {
    // One version is not a choice, and a selector offering it would only take
    // up room and invite a click that changes nothing.
    if page.versions.len() < 2 {
        return String::new();
    }

    let mut out = format!(
        "<div class=\"page-versions\">\n<button class=\"version-menu-toggle\" title=\"Show other \
         versions of page\" aria-expanded=\"false\">{}</button>\n<div class=\"version-menu\">\n",
        text(&page.component.display_version),
    );

    for version in &page.versions {
        let mut classes = String::from("version");

        if version.is_current {
            classes.push_str(" is-current");
        }

        // A version that has not got this page sends the reader to its start
        // page instead, and says so rather than appearing to be the same page
        // somewhere else.
        if version.is_missing {
            classes.push_str(" is-missing");
        }

        let title = if version.is_missing {
            " title=\"This version does not have this page\""
        } else {
            ""
        };

        let _ = writeln!(
            out,
            "<a class=\"{classes}\" href=\"{}\"{title}>{}</a>",
            attr(&version.href),
            text(&version.display_version),
        );
    }

    out.push_str("</div>\n</div>\n");

    out
}

/// The PDF links: this page, and everything in this version of the component.
///
/// Both are `download` links rather than plain ones. A PDF opened in the
/// browser's own viewer is a page the reader has to save a second time, and the
/// point of the button is to come away with the file.
///
/// Each carries the name to save it under, because the URL is not one. Under
/// `html_extension_style: indexify` every page is published as `index.html`
/// inside a directory of its own, so every PDF beside one is `index.pdf`, and a
/// reader who downloads three of them has three files called `index`. The
/// document's own title is what they were looking at.
fn downloads(page: &Page) -> String {
    if page.pdf_url.is_none() && page.manual_url.is_none() {
        return String::new();
    }

    let mut out = String::from("<div class=\"page-pdf\">\n");

    if let Some(url) = &page.pdf_url {
        let _ = writeln!(
            out,
            "<a class=\"pdf-page\" href=\"{}\"{} title=\"Download this page as a PDF\">PDF</a>",
            attr(url),
            download(page.title.as_deref()),
        );
    }

    // The build leaves this unset for a component version of a single page,
    // where the manual and the page are the same document and offering both
    // would be offering the same file twice.
    if let Some(url) = &page.manual_url {
        // "All" says what it is only when it is standing beside the page's
        // own. Alone — which is what a page the typesetter refused gets — it
        // would be a button labelled with the scope of a thing that is not
        // there.
        let label = if page.pdf_url.is_some() { "All" } else { "PDF" };

        let whole = if page.component.is_versioned {
            format!(
                "{} {}",
                page.component.title, page.component.display_version
            )
        } else {
            page.component.title.clone()
        };

        let _ = writeln!(
            out,
            "<a class=\"pdf-manual\" href=\"{}\"{} title=\"Download all of {} as one \
             PDF\">{label}</a>",
            attr(url),
            download(Some(&whole)),
            attr(&page.component.title),
        );
    }

    out.push_str("</div>\n");

    out
}

/// The `download` attribute, with the name to save the file under.
///
/// A title is markup and a file name is not, so the tags come off and the few
/// characters a path cannot hold become spaces. A document with no title at all
/// falls back to a bare `download`, which saves the file under the name it has
/// in the site.
fn download(title: Option<&str>) -> String {
    let Some(title) = title else {
        return " download".to_string();
    };

    let name: String = detag(title)
        .chars()
        .map(|character| match character {
            '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|' => ' ',
            other if other.is_control() => ' ',
            other => other,
        })
        .collect();

    let name = name.trim();

    if name.is_empty() {
        return " download".to_string();
    }

    format!(" download=\"{}.pdf\"", attr(name))
}

/// The "Edit this page" link.
fn edit_link(page: &Page) -> String {
    let Some(url) = &page.edit_url else {
        return String::new();
    };

    format!(
        "<div class=\"edit-this-page\"><a href=\"{}\">Edit this Page</a></div>\n",
        attr(url),
    )
}

/// The article itself: its title, its content, and the way on to the next page.
pub(crate) fn article(page: &Page) -> String {
    let mut classes = String::from("doc");

    if let Some(role) = &page.role {
        classes.push(' ');
        classes.push_str(role);
    }

    let mut out = format!(
        "<article class=\"{}\"{}>\n",
        attr(&classes),
        indexable(page),
    );

    if let Some(title) = &page.title {
        let _ = writeln!(out, "<h1 class=\"page\">{title}</h1>");
    }

    out.push_str(&details(page));
    out.push_str(&page.content);

    if !out.ends_with('\n') {
        out.push('\n');
    }

    out.push_str(&pagination(page));
    out.push_str("</article>\n");

    out
}

/// What tells an indexer that this element is the page, and which component
/// version it belongs to.
///
/// Written whether or not this build wrote an index, because the attributes
/// describe the page rather than the search: run `pagefind` over a site built
/// without the `search` feature and it finds the same body, the same title and
/// the same filters as the built-in index would have.
///
/// The values go in attributes of their own, and are *named* from
/// `data-pagefind-meta`, rather than written into it as `component:Antors`.
/// That form takes everything after its first colon as one literal value, so a
/// component whose title has a comma in it — or a version like `2.0: beta` —
/// would silently become part of the value beside it. Reading each from its own
/// attribute has no such seam, and leaves the escaping to the same function
/// that escapes every other attribute here.
fn indexable(page: &Page) -> String {
    format!(
        " data-pagefind-body data-component=\"{}\" data-version=\"{}\" \
         data-pagefind-meta=\"component[data-component], version[data-version]\" \
         data-pagefind-filter=\"component[data-component], version[data-version]\"",
        attr(&page.component.title),
        attr(&page.component.display_version),
    )
}

/// What the document says about itself, shown between the title and the text.
///
/// The class names are the ones
/// [`adocers-html`](https://crates.io/crates/adocers-html) uses for the same
/// block in a standalone page, so a stylesheet written for one styles the
/// other.
fn details(page: &Page) -> String {
    if page.details.is_empty() {
        return String::new();
    }

    let mut out = String::from("<div class=\"details\">\n");

    for detail in &page.details {
        let values: Vec<String> = detail
            .values
            .iter()
            .map(|value| {
                let text = text(&value.text);

                // A list's entries are separate things, so each is its own
                // mark rather than a run of words with commas between them.
                let inner = if detail.is_list {
                    format!("<span class=\"tag\">{text}</span>")
                } else {
                    text
                };

                match &value.href {
                    Some(href) => format!("<a href=\"{}\">{inner}</a>", attr(href)),
                    None => inner,
                }
            })
            .collect();

        let separator = if detail.is_list { " " } else { ", " };

        // The values are wrapped rather than left loose beside the label: the
        // two are laid out as a pair of columns, and a list long enough to
        // wrap would otherwise put its second line under the *label*.
        let _ = writeln!(
            out,
            "<div class=\"detail\"><span class=\"label\">{}:</span> <span \
             class=\"value\">{}</span></div>",
            text(&detail.label),
            values.join(separator),
        );
    }

    out.push_str("</div>\n");

    out
}

/// The previous and next links at the foot of the article.
fn pagination(page: &Page) -> String {
    if page.previous.is_none() && page.next.is_none() {
        return String::new();
    }

    // Left out of the index: the titles either side of this page are the two
    // pages nearest it in the navigation, and a search that answers with a page
    // because its *neighbour* is called that is a search nobody trusts twice.
    let mut out = String::from("<nav class=\"pagination\" data-pagefind-ignore>\n");

    if let Some(previous) = &page.previous {
        let _ = writeln!(
            out,
            "<span class=\"prev\"><a href=\"{}\">{}</a></span>",
            attr(&previous.href),
            previous.content,
        );
    }

    if let Some(next) = &page.next {
        let _ = writeln!(
            out,
            "<span class=\"next\"><a href=\"{}\">{}</a></span>",
            attr(&next.href),
            next.content,
        );
    }

    out.push_str("</nav>\n");

    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{
        Component,
        Page,
    };

    #[test]
    fn a_pdf_is_saved_under_the_documents_own_name() {
        assert_eq!(
            download(Some("Getting started")),
            r#" download="Getting started.pdf""#
        );

        // A title is markup; a file name is not.
        assert_eq!(
            download(Some("The <code>nav</code> file")),
            r#" download="The nav file.pdf""#
        );

        // And a file name cannot hold a path separator.
        assert_eq!(
            download(Some("Input/output")),
            r#" download="Input output.pdf""#
        );
    }

    #[test]
    fn a_document_with_no_title_keeps_the_name_it_has_in_the_site() {
        assert_eq!(download(None), " download");
        assert_eq!(download(Some("  ")), " download");
    }

    #[test]
    fn a_page_with_no_pdfs_has_no_control_for_them() {
        assert_eq!(downloads(&Page::default()), "");
    }

    #[test]
    fn the_whole_version_is_named_after_the_version_it_is() {
        let page = Page {
            pdf_url: Some("media.pdf".to_string()),
            manual_url: Some("../showcase-2.0.pdf".to_string()),
            title: Some("Images".to_string()),
            component: Component {
                title: "Showcase".to_string(),
                display_version: "2.0".to_string(),
                is_versioned: true,
                ..Component::default()
            },
            ..Page::default()
        };

        let html = downloads(&page);

        assert!(
            html.contains(r#"href="media.pdf" download="Images.pdf""#),
            "{html}"
        );

        assert!(
            html.contains(r#"href="../showcase-2.0.pdf" download="Showcase 2.0.pdf""#),
            "{html}"
        );

        // "All" only says what it is beside the page's own.
        assert!(html.contains(">All</a>"), "{html}");
    }

    #[test]
    fn the_whole_version_stands_on_its_own_when_the_page_has_no_pdf() {
        let page = Page {
            manual_url: Some("showcase-2.0.pdf".to_string()),
            component: Component {
                title: "Showcase".to_string(),
                ..Component::default()
            },
            ..Page::default()
        };

        let html = downloads(&page);

        assert!(!html.contains("pdf-page"), "{html}");
        assert!(html.contains(">PDF</a>"), "{html}");
    }
}
