//! The stylesheet and script the shell needs, compiled in.
//!
//! They are part of the crate rather than fetched at build time, which is the
//! point of not taking an Antora UI bundle: a site builds with nothing on the
//! network and nothing in a cache, and the markup and the CSS that styles it
//! cannot be different versions of each other.

/// One file to write into the site's `_` directory.
#[derive(Clone, Copy, Debug)]
pub struct Asset {
    /// Its path under `_`.
    pub path: &'static str,

    /// Its contents.
    pub contents: &'static [u8],
}

/// The stylesheet.
pub const STYLESHEET: &str = include_str!("../assets/site.css");

/// The script.
pub const SCRIPT: &str = include_str!("../assets/site.js");

/// Every file the shell needs.
pub const ASSETS: &[Asset] = &[
    Asset {
        path: "css/site.css",
        contents: STYLESHEET.as_bytes(),
    },
    Asset {
        path: "js/site.js",
        contents: SCRIPT.as_bytes(),
    },
];
