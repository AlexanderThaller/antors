//! The page shell an article is wrapped in.
//!
//! Everything a reader sees that is not the article itself is here: the navbar,
//! the navigation sidebar, the toolbar with its breadcrumbs and version
//! selector, the outline beside the text, and the stylesheet and script that
//! make them work.
//!
//! # Why this is a crate of its own
//!
//! The shell is built from a [model](model) — a plain description of what the
//! page is and what surrounds it — rather than from the catalog. Nothing here
//! can reach a file, resolve a reference or decide a URL, which means the
//! answer to "what should this page say" is settled before the shell is built,
//! in one place, by code that can be tested without rendering HTML.
//!
//! It is also what makes a second shell possible. The markup here follows
//! Antora's default UI class for class, so a stylesheet written for Antora
//! mostly applies; a build that wants a real Antora UI bundle instead would
//! supply the same [`Page`](model::Page) to a template engine and replace
//! nothing else.

#![warn(clippy::print_stderr, clippy::print_stdout)]

pub mod assets;
pub mod escape;
pub mod model;

mod article;
mod nav;
mod shell;

pub use crate::shell::{
    not_found,
    redirect,
    render,
};
