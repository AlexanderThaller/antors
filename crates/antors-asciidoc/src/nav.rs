//! Turning a `nav.adoc` into the tree the sidebar draws.
//!
//! A navigation file is an ordinary `AsciiDoc` list whose items happen to be
//! cross-references, and that is deliberate: the same `xref:` that works in a
//! page works here, so the navigation cannot drift out of step with what the
//! site contains. It is parsed and resolved exactly as a page is, and then read
//! back out of the rendered markup.
//!
//! Reading it back out of markup rather than out of the parse tree is not
//! laziness. An entry's text is inline `AsciiDoc` — it may be emphasized, it
//! may carry an icon, it may be an attribute reference — and it has to reach
//! the sidebar rendered. The link, if there is one, is then the anchor wrapped
//! around it.

use std::{
    rc::Rc,
    sync::Arc,
};

use antors_content::{
    Catalog,
    ComponentVersion,
};
use antors_model::{
    resource::Key,
    url::relativize,
};
use asciidoc_parser::{
    Parser,
    SafeMode,
    blocks::{
        Block,
        FindBlocks,
        IsBlock,
        ListBlock,
        ListItem,
        SimpleBlockStyle,
    },
};

use crate::links::{
    Links,
    Shared,
};

/// What kind of place a navigation entry points at.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum LinkType {
    /// A page in this site, whose URL is relativized per page.
    Internal,

    /// Somewhere else entirely, whose URL is written as it stands.
    External,

    /// A fragment of the page the reader is already on.
    Fragment,
}

/// One entry of the navigation tree.
#[derive(Clone, Debug)]
pub struct Item {
    /// The entry's text, already rendered.
    pub content: String,

    /// Where it points, as an absolute site path for an internal link.
    pub url: Option<String>,

    /// What kind of place that is.
    pub link_type: LinkType,

    /// The entries nested beneath it.
    pub items: Vec<Item>,
}

impl Item {
    /// The href this entry should carry on a page published at `page_url`.
    pub fn href(&self, page_url: &str) -> Option<String> {
        let url = self.url.as_ref()?;

        Some(match self.link_type {
            LinkType::Internal => relativize(page_url, url),
            LinkType::External | LinkType::Fragment => url.clone(),
        })
    }

    /// Whether this entry, or anything beneath it, is the page at `page_url`.
    ///
    /// The sidebar opens every list on the path to the current page, so this
    /// answers for a whole subtree rather than for one entry.
    pub fn contains(&self, page_url: &str) -> bool {
        if self.url.as_deref() == Some(page_url) && self.link_type == LinkType::Internal {
            return true;
        }

        self.items.iter().any(|item| item.contains(page_url))
    }
}

/// Read every navigation file a component version names.
///
/// Each file contributes one top-level entry, whose text is the file's list
/// title and whose children are its list. A file with no title contributes an
/// entry with no text, which the sidebar draws as an unlabelled group — which
/// is how a component whose navigation is one flat list gets one.
pub fn build(catalog: &Arc<Catalog>, component_version: &ComponentVersion) -> Vec<Item> {
    component_version
        .nav
        .iter()
        .filter_map(|key| read(catalog, key))
        .collect()
}

/// Read one navigation file.
fn read(catalog: &Arc<Catalog>, key: &Key) -> Option<Item> {
    let file = catalog.get(key)?;
    let source = std::fs::read_to_string(&file.path).ok()?;

    // One resolver, in all three roles: the parser holds it as a path
    // resolver, and the resolution pass below uses the same object to resolve
    // references and to render them.
    let links = Rc::new(Links::absolute(Arc::clone(catalog), key.clone()));

    let mut parser = Parser::default()
        .with_safe_mode(SafeMode::Safe)
        .with_primary_file_name(&file.relative_src_path)
        .with_path_resolver(Shared(Rc::clone(&links)));

    let mut document = parser.parse_deferred(&source);
    links.adopt(document.catalog().clone());
    document.resolve_references(&*links, &*links, &parser);

    let list = document.child_blocks().find_map(|block| match block {
        Block::List(list) => Some(list),
        _ => None,
    })?;

    Some(Item {
        content: list.title().unwrap_or_default().to_string(),
        url: None,
        link_type: LinkType::Internal,
        items: items(list),
    })
}

/// Read one list into entries.
fn items<'src>(list: &'src ListBlock<'src>) -> Vec<Item> {
    list.child_blocks()
        .filter_map(|block| match block {
            Block::ListItem(item) => Some(entry(item)),
            _ => None,
        })
        .collect()
}

/// Read one list item into an entry.
fn entry<'src>(item: &'src ListItem<'src>) -> Item {
    let children: Vec<&'src Block<'src>> = item.child_blocks().collect();

    let principal = children.iter().find_map(|block| match block {
        Block::Simple(simple) if simple.style() == SimpleBlockStyle::Paragraph => {
            Some(simple.content().rendered_html())
        }

        _ => None,
    });

    let nested = children
        .iter()
        .filter_map(|block| match block {
            Block::List(list) => Some(items(list)),
            _ => None,
        })
        .flatten()
        .collect();

    let (content, url, link_type) = split_anchor(principal.unwrap_or_default());

    Item {
        content,
        url,
        link_type,
        items: nested,
    }
}

/// Pull the entry's text and destination out of its rendered markup.
///
/// An entry that is a link is one anchor wrapping the whole text; an entry that
/// is a heading is the text on its own. Anything else — a link with words
/// beside it — is treated as text, because the sidebar has one link per entry
/// and guessing which one was meant would be worse than showing none.
fn split_anchor(html: &str) -> (String, Option<String>, LinkType) {
    let html = html.trim();

    let text = |html: &str| (html.to_string(), None, LinkType::Internal);

    if !html.starts_with("<a ") || !html.ends_with("</a>") {
        return text(html);
    }

    let Some(open_end) = html.find('>') else {
        return text(html);
    };

    let inner = &html[open_end + 1..html.len() - "</a>".len()];

    // A second anchor means the entry is not one link but a sentence
    // containing links.
    if inner.contains("<a ") {
        return text(html);
    }

    let Some(href) = attribute(&html[..open_end], "href") else {
        return text(html);
    };

    let link_type = if href.starts_with('#') {
        LinkType::Fragment
    } else if href.starts_with('/') {
        LinkType::Internal
    } else {
        LinkType::External
    };

    (inner.to_string(), Some(href), link_type)
}

/// One attribute's value out of an opening tag.
fn attribute(tag: &str, name: &str) -> Option<String> {
    let needle = format!("{name}=\"");
    let start = tag.find(&needle)? + needle.len();
    let end = tag[start..].find('"')? + start;

    Some(tag[start..end].to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_lone_anchor_becomes_a_link() {
        let (content, url, link_type) =
            split_anchor(r#"<a href="/a/b.html" class="xref page">Blocks</a>"#);

        assert_eq!(content, "Blocks");
        assert_eq!(url.as_deref(), Some("/a/b.html"));
        assert_eq!(link_type, LinkType::Internal);
    }

    #[test]
    fn an_external_anchor_is_told_apart_by_its_href() {
        let (_, url, link_type) = split_anchor(r#"<a href="https://example.org">Docs</a>"#);

        assert_eq!(url.as_deref(), Some("https://example.org"));
        assert_eq!(link_type, LinkType::External);
    }

    #[test]
    fn plain_text_is_a_heading_rather_than_a_link() {
        let (content, url, _) = split_anchor("AsciiDoc");

        assert_eq!(content, "AsciiDoc");
        assert_eq!(url, None);
    }

    #[test]
    fn formatted_text_keeps_its_markup() {
        let (content, url, _) = split_anchor("<strong>New</strong> features");

        assert_eq!(content, "<strong>New</strong> features");
        assert_eq!(url, None);
    }

    #[test]
    fn an_entry_with_a_link_inside_a_sentence_is_text() {
        let (_, url, _) =
            split_anchor(r#"See <a href="/a.html">this</a> and <a href="/b.html">that</a>"#);

        assert_eq!(url, None);
    }

    #[test]
    fn an_item_knows_when_the_current_page_is_beneath_it() {
        let tree = Item {
            content: "Guide".to_string(),
            url: None,
            link_type: LinkType::Internal,
            items: vec![Item {
                content: "Start".to_string(),
                url: Some("/c/1.0/start.html".to_string()),
                link_type: LinkType::Internal,
                items: Vec::new(),
            }],
        };

        assert!(tree.contains("/c/1.0/start.html"));
        assert!(!tree.contains("/c/1.0/other.html"));
    }
}
