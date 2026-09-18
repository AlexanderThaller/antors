//! Building a whole site.
//!
//! This is the crate that runs the build: it collects the content, renders
//! every page, wraps each in the shell, and writes the result. Everything it
//! decides it decides once — where a page goes, what its neighbours are, which
//! version is latest — so the rest of the build has facts to work from rather
//! than questions to re-answer.
//!
//! # The shape of a build
//!
//! ```text
//! playbook ──► aggregate ──► catalog
//!                              │
//!                      titles  │  every page's name, because a reference
//!                       pass   │  with no text of its own shows the target's
//!                              ▼
//!                           navigation ──► page order, breadcrumbs
//!                              │
//!                       render │  one page at a time: parse, resolve,
//!                        pass  │  render, wrap in the shell, write
//!                              ▼
//!                            site
//! ```

#![warn(clippy::print_stderr, clippy::print_stdout)]

pub mod build;
pub mod report;

mod page;
mod pdf;
mod search;
mod sitemap;
mod tags;
mod write;

pub use crate::{
    build::{
        Build,
        BuildError,
        Options,
    },
    report::{
        Problem,
        Report,
    },
};
