//! The stylesheet and script the shell needs.
//!
//! They are part of the crate rather than fetched at build time, which is the
//! point of not taking an Antora UI bundle: a site builds with nothing on the
//! network and nothing in a cache, and the markup and the CSS that styles it
//! cannot be different versions of each other.
//!
//! # The stylesheet is composed, not written
//!
//! Only the *shell* is this project's: the navbar, the navigation tree, the
//! toolbar, the outline beside the text. The article is styled by
//! [`adocers-html`], and its stylesheet is used rather than imitated — see
//! [`stylesheet`] for why a copy would not merely drift but break the diagrams.
//!
//! [`adocers-html`]: https://crates.io/crates/adocers-html

use std::borrow::Cow;

/// One file to write into the site's `_` directory.
#[derive(Clone, Debug)]
pub struct Asset {
    /// Its path under `_`.
    pub path: &'static str,

    /// Its contents.
    pub contents: Cow<'static, [u8]>,
}

/// Where the document sits in a page this crate builds.
///
/// The back end's rules are nested under it, which is what keeps them off the
/// navbar and the navigation tree — both of which contain inline-rendered
/// `AsciiDoc` and would otherwise be styled as if they were prose.
const ARTICLE: &str = "article.doc";

/// The shell's own rules.
pub const SHELL: &str = include_str!("../assets/shell.css");

/// The script.
pub const SCRIPT: &str = include_str!("../assets/site.js");

/// The whole stylesheet: the back end's, and the shell's.
///
/// Three parts, in this order and for these reasons.
///
/// The back end's **custom properties** come first, at the top level. They are
/// not a palette this project happens to share — a mermaid diagram drawn into
/// the page carries theme overrides written against them, looked up from inside
/// the `<svg>`. Declared out of their reach, or under different names, every
/// one of those overrides resolves to nothing and `fill` falls back to its
/// initial value: a diagram of solid black boxes, with nothing in the page to
/// say why. That is not a hypothetical; it is what a hand-written palette did
/// here.
///
/// The **shell's** rules come next, so the article's can override them where
/// the two meet.
///
/// The back end's **document rules** come last, nested under
/// [`ARTICLE`]. Nesting rather than rewriting each selector is what makes this
/// reuse rather than a copy: the block goes in untouched, so there is no
/// version of it here to fall behind the back end's own.
pub fn stylesheet() -> String {
    format!(
        "{}\n{SHELL}\n{ARTICLE} {{\n{}\n}}\n",
        adocers_html::stylesheet_variables(),
        adocers_html::document_stylesheet(),
    )
}

/// Every file the shell needs.
pub fn assets() -> Vec<Asset> {
    vec![
        Asset {
            path: "css/site.css",
            contents: Cow::Owned(stylesheet().into_bytes()),
        },
        Asset {
            path: "js/site.js",
            contents: Cow::Borrowed(SCRIPT.as_bytes()),
        },
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_back_ends_properties_are_declared_where_a_diagram_can_reach_them() {
        let css = stylesheet();
        let nested = css.find(ARTICLE).expect("the article block is there");

        for property in ["--code-bg", "--rule", "--fg", "--accent", "--sidebar-bg"] {
            let declared = css
                .find(&format!("{property}:"))
                .unwrap_or_else(|| panic!("`{property}` is not declared"));

            assert!(
                declared < nested,
                "`{property}` is declared inside `{ARTICLE}`, where a diagram's own theme cannot \
                 reach it"
            );
        }
    }

    #[test]
    fn the_back_ends_page_rules_are_left_out() {
        let css = stylesheet();

        // The frame of a standalone page describes a page this crate is not
        // making; taking it would put a second, narrower column inside the one
        // the shell already lays out.
        for selector in ["#header,", "#content,", "body.toc2"] {
            assert!(
                !css.contains(selector),
                "`{selector}` belongs to a standalone page, not to a site"
            );
        }
    }

    #[test]
    fn the_article_rules_are_nested_rather_than_rewritten() {
        let css = stylesheet();

        // Taken verbatim: if this had to rewrite each selector, it would be a
        // copy of the back end's stylesheet with a different bug in it.
        assert!(css.contains(adocers_html::document_stylesheet()));
    }
}
