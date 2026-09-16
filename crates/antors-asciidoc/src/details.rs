//! The facts a document states about itself.
//!
//! A document header carries two sorts of thing, and only one of them is for
//! the reader. `:sectnums:` and `:page-role:` are instructions to the renderer;
//! an author, a revision, a status and a set of tags are *about the document*,
//! and a reader who opens a design note wants to know when it was written and
//! whether it still stands.
//!
//! Which attributes are facts, and what each is called, is
//! [`adocers-render-core`]'s answer rather than one of this crate's own — so a
//! page here and a PDF of the same document say the same things under the same
//! labels.
//!
//! [`adocers-render-core`]: https://crates.io/crates/adocers-render-core

use adocers_render_core::metadata::{
    displayed_as,
    is_list,
    label_for,
};
use asciidoc_parser::{
    Document,
    document::InterpretedValue,
};

/// One labelled fact about a document.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Detail {
    /// The attribute it came from, with any `page-` prefix already dropped.
    ///
    /// The shell reads this to know that `tags` are worth linking somewhere and
    /// that `author` is not.
    pub name: String,

    /// What the reader sees in front of it.
    pub label: String,

    /// Its value: one entry, or several when the attribute is a list.
    pub values: Vec<DetailValue>,

    /// Whether those entries are separate things rather than one sentence.
    pub is_list: bool,
}

/// One value of a labelled fact.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DetailValue {
    /// What the reader sees.
    pub text: String,

    /// An address to write to, for a value that names a person.
    ///
    /// The shell turns this into a `mailto:` link on the name, rather than
    /// printing the address beside it: a reader wants the person, and a bare
    /// address in a published page is a gift to whoever is harvesting them.
    pub mailto: Option<String>,
}

impl DetailValue {
    /// A value that is only text.
    fn text(value: impl Into<String>) -> Self {
        Self {
            text: value.into(),
            mailto: None,
        }
    }
}

impl Detail {
    /// A fact with a single value.
    fn single(name: &str, label: &str, value: impl Into<String>) -> Self {
        Self {
            name: name.to_string(),
            label: label.to_string(),
            values: vec![DetailValue::text(value)],
            is_list: false,
        }
    }
}

/// Every fact `document` states about itself, in the order it should be shown.
///
/// Authorship first, then the revision, then whatever else the header said —
/// which is the order a reader scans, and the order
/// [`adocers-html`](https://crates.io/crates/adocers-html) puts them in.
pub fn of(document: &Document<'_>) -> Vec<Detail> {
    let mut details = Vec::new();

    details.extend(authors(document));
    details.extend(revision(document));
    details.extend(header_metadata(document));

    // The remark describes the revision rather than naming part of it, so it
    // comes after everything it might be commenting on.
    if let Some(remark) = attribute(document, "revremark") {
        details.push(Detail::single("revremark", "Remark", remark));
    }

    details
}

/// The `Author` line, or nothing when the document names nobody.
fn authors(document: &Document<'_>) -> Option<Detail> {
    let authors = document.authors();

    if authors.is_empty() {
        return None;
    }

    let values: Vec<DetailValue> = authors
        .iter()
        .map(|author| DetailValue {
            text: author.name().to_string(),
            mailto: author.email().map(str::to_string),
        })
        .collect();

    // One author is a name; several are a list, so each is shown as its own
    // thing rather than run into a sentence with commas a reader has to parse.
    let is_list = values.len() > 1;

    Some(Detail {
        name: "author".to_string(),
        label: if is_list { "Authors" } else { "Author" }.to_string(),
        values,
        is_list,
    })
}

/// The `Version` and `Date` lines.
///
/// These read the attributes rather than the revision line, because an explicit
/// `v1.0, 2026-09-10` line sets them too — so the two ways of writing a
/// revision are one case here rather than two, and either may appear without
/// the other.
fn revision(document: &Document<'_>) -> Vec<Detail> {
    let mut details = Vec::new();

    if let Some(number) = attribute(document, "revnumber") {
        details.push(Detail::single("revnumber", "Version", number));
    }

    if let Some(date) = attribute(document, "revdate") {
        details.push(Detail::single("revdate", "Date", date));
    }

    details
}

/// Whatever else the header said that is a fact rather than an instruction.
fn header_metadata(document: &Document<'_>) -> Vec<Detail> {
    let mut details = Vec::new();

    for attribute in document.header().attributes() {
        let name = attribute.name().data();

        let Some(shown) = displayed_as(name) else {
            continue;
        };

        if is_directive(shown) {
            continue;
        }

        // An attribute set without a value — `:sectnums:` — says something to
        // the renderer and nothing to a reader.
        let InterpretedValue::Value(value) = attribute.value() else {
            continue;
        };

        if value.is_empty() {
            continue;
        }

        let list = is_list(shown);

        details.push(Detail {
            name: shown.to_string(),
            label: label_for(shown),
            values: if list {
                split(value).into_iter().map(DetailValue::text).collect()
            } else {
                vec![DetailValue::text(value.clone())]
            },
            is_list: list,
        });
    }

    details
}

/// Antora's own `page-` attributes, which tell the build what to do rather than
/// the reader what this is.
///
/// The `page-` namespace is mixed: `page-tags` is metadata and `page-role` is a
/// class name. An allowlist would be the safer shape, except that the whole
/// point of the namespace is that an author may put their own things in it — so
/// what is excluded is the set Antora itself defines, and everything an author
/// invented is shown.
const DIRECTIVES: &[&str] = &[
    "aliases",
    "edit-url",
    "layout",
    "pagination",
    "partial",
    "role",
    "toclevels",
];

/// Whether a `page-` attribute is one of Antora's own.
fn is_directive(name: &str) -> bool {
    DIRECTIVES.contains(&name)
        // The attributes the build sets to say where a page sits. They are set
        // on the parser rather than written in a header, so they should not
        // reach here at all — but a page is free to write one, and a header
        // reading `Component name: showcase` would be noise.
        || name.starts_with("component-")
        || name.starts_with("origin-")
        || matches!(
            name,
            "version" | "display-version" | "module" | "relative-src-path"
        )
}

/// Split a list attribute into its separate things.
///
/// Commas are the `AsciiDoc` convention, but an author writing `Tags: a b c`
/// means three tags rather than one — so a value with no comma in it is split
/// on whitespace instead.
fn split(value: &str) -> Vec<String> {
    let separator = if value.contains(',') { ',' } else { ' ' };

    value
        .split(separator)
        .map(str::trim)
        .filter(|entry| !entry.is_empty())
        .map(str::to_string)
        .collect()
}

/// One document attribute's value, if it is set to anything.
fn attribute(document: &Document<'_>, name: &str) -> Option<String> {
    match document.attribute_value(name) {
        InterpretedValue::Value(value) => Some(value.clone()),
        InterpretedValue::Set | InterpretedValue::Unset => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_comma_list_splits_on_commas() {
        assert_eq!(
            split("asciidoc, rendering, showcase"),
            ["asciidoc", "rendering", "showcase"]
        );
    }

    #[test]
    fn a_list_with_no_commas_splits_on_spaces() {
        // `Tags: asciidoc rendering showcase` is three tags, not one.
        assert_eq!(
            split("asciidoc rendering showcase"),
            ["asciidoc", "rendering", "showcase"]
        );
    }

    #[test]
    fn a_lone_entry_stays_one_entry() {
        assert_eq!(split("asciidoc"), ["asciidoc"]);
        assert_eq!(split("a-long-tag"), ["a-long-tag"]);
    }

    #[test]
    fn only_a_list_attribute_is_split() {
        // `split` is reached only for the attributes `is_list` names, so a
        // `:status: living document` is never cut in half — which it would be,
        // since a space is a separator in a list.
        assert!(is_list("tags"));
        assert!(is_list("keywords"));
        assert!(!is_list("status"));
        assert!(!is_list("category"));
    }

    #[test]
    fn antoras_own_page_attributes_are_not_facts() {
        for name in [
            "role",
            "aliases",
            "toclevels",
            "component-name",
            "origin-url",
        ] {
            assert!(is_directive(name), "`page-{name}` is not a fact");
        }
    }

    #[test]
    fn an_authors_own_page_attribute_is() {
        for name in ["tags", "status", "reviewed-by"] {
            assert!(!is_directive(name), "`page-{name}` should be shown");
        }
    }
}
