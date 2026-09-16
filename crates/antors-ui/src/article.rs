//! The article and the toolbar above it.

use std::fmt::Write as _;

use crate::{
    escape::{
        attr,
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

    let mut out = format!("<article class=\"{}\">\n", attr(&classes));

    if let Some(title) = &page.title {
        let _ = writeln!(out, "<h1 class=\"page\">{title}</h1>");
    }

    out.push_str(&page.content);

    if !out.ends_with('\n') {
        out.push('\n');
    }

    out.push_str(&pagination(page));
    out.push_str("</article>\n");

    out
}

/// The previous and next links at the foot of the article.
fn pagination(page: &Page) -> String {
    if page.previous.is_none() && page.next.is_none() {
        return String::new();
    }

    let mut out = String::from("<nav class=\"pagination\">\n");

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
