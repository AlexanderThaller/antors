//! Collecting content sources into one catalog of resources.
//!
//! A build begins with directories and ends with a catalog: every file found
//! under every content source, keyed by the [resource ID] it will be addressed
//! by, grouped into the component versions that own them. Nothing downstream
//! looks at a directory again — a reference is resolved against the catalog, an
//! include is read from the catalog, and a page's URL is the catalog's answer.
//!
//! That indirection is what makes the rest of the build indifferent to where
//! content came from. This crate collects from a worktree on disk; a later one
//! that collects from a git object database has only to produce the same
//! [`Catalog`].
//!
//! [resource ID]: antors_model::ResourceId

#![warn(clippy::print_stderr, clippy::print_stdout)]

pub mod aggregate;
pub mod catalog;
pub mod contents;
pub mod git;
pub mod origin;

pub use crate::{
    catalog::{
        Catalog,
        Component,
        ComponentVersion,
        SourceFile,
    },
    contents::Contents,
    origin::Origin,
};
