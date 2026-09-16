//! Antora's domain model, and nothing that reads a disk or renders a page.
//!
//! Antora names every file in a site with a [resource ID], and almost
//! everything else in the model follows from that: which component and version
//! a file belongs to, which module inside it, and which *family* — a page, a
//! partial, an example, an image, an attachment. The URL a file is published
//! at is computed from the same five parts, so a reference written in one page
//! and the file it points at are two views of one identifier.
//!
//! This crate is that identifier, the two configuration files that populate it
//! ([`Playbook`] and [`Descriptor`]), and the URL arithmetic that turns it into
//! a link. Collecting the files is [`antors-content`]'s job and rendering them
//! is [`antors-asciidoc`]'s; neither is here, so this crate can be reasoned
//! about — and tested — without a site on disk.
//!
//! [resource ID]: resource::ResourceId
//! [`antors-content`]: https://crates.io/crates/antors-content
//! [`antors-asciidoc`]: https://crates.io/crates/antors-asciidoc

#![warn(clippy::print_stderr, clippy::print_stdout)]

pub mod descriptor;
pub mod playbook;
pub mod resource;
pub mod url;
pub mod version;

pub use crate::{
    descriptor::Descriptor,
    playbook::Playbook,
    resource::{
        Family,
        ResourceId,
    },
};
