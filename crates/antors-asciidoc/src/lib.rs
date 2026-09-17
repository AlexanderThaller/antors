//! Turning one page of an Antora site into HTML.
//!
//! The `AsciiDoc` itself is parsed by [`asciidoc-parser`] and rendered by
//! [`adocers-html`]; neither knows what a component or a module is, and neither
//! needs to. What this crate supplies is the three seams where Antora's model
//! has to reach into a parse:
//!
//! - [`include`] — an `include::partial$…[]` names a resource, not a path, and
//!   the catalog is what turns one into the other.
//! - [`links`] — an `xref:` may cross a module, a component or a version, and
//!   only the whole site knows where it lands.
//! - [`attributes`] — a page is handed the `page-*` attributes that say where
//!   it sits, and the `imagesdir` that makes its images resolve.
//!
//! # Two passes, not one
//!
//! A reference with no text of its own shows the *target page's* title, so no
//! page's references can be resolved until every page's title is known. A build
//! therefore reads each page twice: [`title_of`] first, for every page, and
//! [`Renderer::render`] second. The alternative — holding every parsed document
//! in memory between the passes — costs more than parsing twice and gives the
//! same answer.
//!
//! [`asciidoc-parser`]: https://crates.io/crates/asciidoc-parser
//! [`adocers-html`]: https://crates.io/crates/adocers-html

#![warn(clippy::print_stderr, clippy::print_stdout)]

pub mod attributes;
pub mod details;
pub mod include;
pub mod links;
pub mod nav;

mod render;

pub use crate::{
    attributes::Attributes,
    details::{
        Detail,
        DetailValue,
    },
    render::{
        Header,
        Options,
        Rendered,
        Renderer,
        Warning,
        title_of,
    },
};
