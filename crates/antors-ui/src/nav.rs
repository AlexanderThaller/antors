//! The navigation sidebar: the component's own menu, and the way out of it.

use std::fmt::Write as _;

use crate::{
    escape::{
        attr,
        text,
    },
    model::{
        ComponentEntry,
        NavItem,
        Page,
    },
};

/// The whole sidebar, both panels.
pub(crate) fn sidebar(page: &Page) -> String {
    let mut out = String::new();

    let _ = write!(
        out,
        "<div class=\"nav-container\" data-component=\"{}\" data-version=\"{}\">\n<aside \
         class=\"nav\">\n<div class=\"panels\">\n",
        attr(&page.component.name),
        attr(&page.component.version),
    );

    out.push_str(&menu(page));
    out.push_str(&explore(page));

    out.push_str("</div>\n</aside>\n</div>\n");

    out
}

/// The panel showing this component version's navigation.
fn menu(page: &Page) -> String {
    let mut out = String::from(
        "<div class=\"nav-panel-menu is-active\" data-panel=\"menu\">\n<nav class=\"nav-menu\">\n",
    );

    let _ = writeln!(
        out,
        "<h3 class=\"title\"><a href=\"{}\">{}</a></h3>",
        attr(&page.component.url),
        text(&page.component.title),
    );

    out.push_str(&list(&page.navigation));

    out.push_str("</nav>\n</div>\n");

    out
}

/// One level of the navigation tree.
fn list(items: &[NavItem]) -> String {
    if items.is_empty() {
        return String::new();
    }

    let mut out = String::from("<ul class=\"nav-list\">\n");

    for item in items {
        out.push_str(&entry(item));
    }

    out.push_str("</ul>\n");

    out
}

/// One navigation entry, and everything beneath it.
fn entry(item: &NavItem) -> String {
    let mut classes = String::from("nav-item");

    if item.is_current {
        classes.push_str(" is-current-page");
    }

    // A list on the path to the page being read starts open, so the reader can
    // see where they are without opening anything.
    if item.is_on_path && !item.items.is_empty() {
        classes.push_str(" is-open");
    }

    let mut out = format!("<li class=\"{classes}\">\n");

    if !item.items.is_empty() {
        out.push_str(
            "<button class=\"nav-item-toggle\" aria-label=\"Expand or collapse this \
             section\"></button>\n",
        );
    }

    match &item.href {
        Some(href) => {
            let _ = writeln!(
                out,
                "<a class=\"nav-link\" href=\"{}\">{}</a>",
                attr(href),
                item.content,
            );
        }

        // An entry with no link names a group. It still needs an element of
        // its own, because the toggle beside it has to have something to sit
        // next to and the reader has to have something to read.
        None if !item.content.is_empty() => {
            let _ = writeln!(out, "<span class=\"nav-text\">{}</span>", item.content);
        }

        None => {}
    }

    out.push_str(&list(&item.items));
    out.push_str("</li>\n");

    out
}

/// The panel listing every component, for moving between them.
fn explore(page: &Page) -> String {
    let mut out = String::from("<div class=\"nav-panel-explore\" data-panel=\"explore\">\n");

    let _ = write!(
        out,
        "<div class=\"context\"><button class=\"back\" aria-label=\"Back to this component's \
         navigation\"></button><span class=\"title\">{}</span>",
        text(&page.component.title),
    );

    if page.component.is_versioned {
        let _ = write!(
            out,
            "<span class=\"version\">{}</span>",
            text(&page.component.display_version),
        );
    }

    out.push_str("</div>\n<ul class=\"components\">\n");

    for component in &page.components {
        out.push_str(&component_entry(component));
    }

    out.push_str("</ul>\n</div>\n");

    out
}

/// One component in the explore panel, with its versions.
fn component_entry(component: &ComponentEntry) -> String {
    let classes = if component.is_current {
        "component is-current"
    } else {
        "component"
    };

    let mut out = format!(
        "<li class=\"{classes}\">\n<div class=\"title\"><a href=\"{}\">{}</a></div>\n",
        attr(&component.href),
        text(&component.title),
    );

    if !component.versions.is_empty() {
        out.push_str("<ul class=\"versions\">\n");

        for version in &component.versions {
            let mut classes = String::from("version");

            if version.is_current {
                classes.push_str(" is-current");
            }

            if version.is_latest {
                classes.push_str(" is-latest");
            }

            let _ = writeln!(
                out,
                "<li class=\"{classes}\"><a href=\"{}\">{}</a></li>",
                attr(&version.href),
                text(&version.display_version),
            );
        }

        out.push_str("</ul>\n");
    }

    out.push_str("</li>\n");

    out
}
