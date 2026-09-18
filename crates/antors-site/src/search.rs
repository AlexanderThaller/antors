//! Writing the site's search index.
//!
//! The index is [pagefind]'s, built in this process as the pages are rendered
//! and written into the site beside the stylesheet and the script. Nothing is
//! fetched: the index format, the JavaScript that reads it and the wasm module
//! that searches it are all inside the `pagefind` crate, which is what makes a
//! searchable site something this build can produce on its own — the same
//! property that decided the page shell.
//!
//! # Why the pages are indexed, and not the sources
//!
//! What a reader searches for is what they read, and by the time a page is
//! written that is the only place it exists. An `include::` has been resolved,
//! an attribute substituted, a table laid out, a `xref:` given the text of the
//! page it points at. Indexing the `AsciiDoc` instead would mean re-deciding
//! every one of those, differently, in a second implementation — and finding
//! `{product-name}` in the index where the page says `Antors`.
//!
//! So the input here is each page's finished HTML, marked up by [`antors_ui`]
//! with what it is: which element is the document rather than the navigation
//! beside it, which component version it belongs to, and what not to read at
//! all.
//!
//! # What a result points at
//!
//! Each page is indexed under its path in the output directory —
//! `component/version/page.html`, with no leading slash — and the script in the
//! browser joins that to the path from *the page being read* back to the root.
//! A site built this way does not have to know its own address: it searches the
//! same from a subdirectory, from a branch preview, and from a copy on a disk.
//!
//! [`antors_ui`]: https://crates.io/crates/antors-ui
//! [pagefind]: https://pagefind.app

pub(crate) use index::Index;

/// The index a build compiled without the `search` feature does not make.
///
/// An empty type rather than a missing one, so that the shape of a build does
/// not change with the feature: the same code opens an index, fills it from the
/// pages and writes it either way, and what the feature decides is only whether
/// there can be one. Nothing here can be called, because nothing can be
/// constructed — which is the compiler's way of saying the same thing.
#[cfg(not(feature = "search"))]
mod index {
    use crate::{
        build::BuildError,
        report::Report,
        write::Writer,
    };

    /// An index that cannot exist.
    #[derive(Debug)]
    pub(crate) enum Index {}

    impl Index {
        /// Add one page, as it was written.
        pub(crate) fn add(&mut self, _url: &str, _html: &str) -> Result<(), String> {
            match *self {}
        }

        /// Build the index and write it into the site.
        pub(crate) fn write(
            self,
            _writer: &mut Writer,
            _report: &mut Report,
        ) -> Result<(), BuildError> {
            match self {}
        }
    }
}

#[cfg(feature = "search")]
mod index {
    use std::path::Path;

    use pagefind::api::PagefindIndex;

    use crate::{
        build::BuildError,
        report::Report,
        write::Writer,
    };

    /// What a build reports a problem with the index against.
    const SEARCH: &str = "search";

    /// Where the search bundle is written, under the output root.
    ///
    /// Beside the rest of the UI, under `_`, rather than at the site root where
    /// pagefind's own command would put it: the index is part of the page
    /// shell, and an output directory has enough at its top level already.
    const UI_DIR: &str = "_";

    /// The directory the index's own files go in, under [`UI_DIR`].
    ///
    /// Pagefind hands its files back named relative to a bundle directory and
    /// leaves where that is to whoever asked, so this is the only place the
    /// name is decided — and the shell writes the same path into every search
    /// box as the place to look.
    const BUNDLE_DIR: &str = "pagefind";

    /// The parts of the bundle that a site with its own search box has no use
    /// for.
    ///
    /// Pagefind ships three ready-made interfaces along with the index. The
    /// shell uses none of them — it reads the index through the small module
    /// that stays — and half a megabyte of unreferenced JavaScript in every
    /// built site is half a megabyte somebody has to explain.
    ///
    /// A list of what to leave out rather than what to take, so that a file
    /// this code has never heard of is still written: the cost of a spare file
    /// is a wasted kilobyte, and the cost of a missing one is a search box that
    /// does not work.
    const UNUSED: [&str; 3] = [
        "pagefind-ui.",
        "pagefind-modular-ui.",
        "pagefind-component-ui.",
    ];

    /// The index, from the first page added to the files written.
    ///
    /// Pages are added one at a time, as each is rendered, rather than kept and
    /// indexed together at the end. Indexing a page costs a fraction of what
    /// rendering it did, and the alternative is holding the finished HTML of
    /// every page in the site in memory for the length of the build to save
    /// that fraction.
    pub(crate) struct Index {
        /// The runtime pagefind's API is asked on.
        ///
        /// Pagefind is async and a build is not, so the index carries a runtime
        /// of its own rather than the build growing one.
        runtime: tokio::runtime::Runtime,

        /// What is being filled.
        pagefind: PagefindIndex,
    }

    /// Written out rather than derived: pagefind's index is not `Debug`, and
    /// the inside of a search index is not what somebody printing a build's
    /// state wants to read anyway.
    impl std::fmt::Debug for Index {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            f.debug_struct("Index").finish_non_exhaustive()
        }
    }

    impl Index {
        /// An empty index.
        pub(crate) fn new() -> Result<Self, String> {
            let runtime = tokio::runtime::Builder::new_current_thread()
                .enable_time()
                .build()
                .map_err(|error| error.to_string())?;

            let pagefind = PagefindIndex::new(None).map_err(|error| error.to_string())?;

            Ok(Self { runtime, pagefind })
        }

        /// Add one page, as it was written.
        ///
        /// `url` is where the page was published, relative to the output
        /// directory, which is what a result for it points at.
        pub(crate) fn add(&mut self, url: &str, html: &str) -> Result<(), String> {
            self.runtime
                .block_on(self.pagefind.add_html_file(
                    None,
                    Some(url.to_string()),
                    html.to_string(),
                ))
                .map(|_| ())
                .map_err(|error| error.to_string())
        }

        /// Build the index and write it into the site.
        ///
        /// An index that could not be built is reported and the build goes on.
        /// The site is still a site; it is the search box that will not work,
        /// and saying so is more use than failing a build that has already
        /// written every page.
        pub(crate) fn write(
            mut self,
            writer: &mut Writer,
            report: &mut Report,
        ) -> Result<(), BuildError> {
            let files = match self.runtime.block_on(self.pagefind.get_files()) {
                Ok(files) => files,

                Err(error) => {
                    report.error(SEARCH, format!("the search index was not built: {error}"));

                    return Ok(());
                }
            };

            for file in files {
                if unused(&file.filename) {
                    continue;
                }

                match bundle_path(&file.filename) {
                    Ok(path) => writer.file(&path, &file.contents)?,
                    Err(problem) => report.error(SEARCH, problem),
                }
            }

            Ok(())
        }
    }

    /// Whether a file of the bundle is one the shell has no use for.
    fn unused(path: &Path) -> bool {
        let Some(name) = path.file_name().map(std::ffi::OsStr::to_string_lossy) else {
            return false;
        };

        UNUSED.iter().any(|unused| name.starts_with(unused))
    }

    /// Where a file of the bundle goes in the site.
    ///
    /// Pagefind names each file relative to the bundle directory and leaves
    /// placing it to whoever asked for the files, so this is where the bundle
    /// is put somewhere. What is checked is that the name *is* relative and
    /// stays inside: it is joined onto the output directory, and a path that
    /// climbed out of it would write over whatever it found there.
    fn bundle_path(path: &Path) -> Result<String, String> {
        let name = path.to_string_lossy().replace('\\', "/");

        let escapes = name.is_empty()
            || name.starts_with('/')
            || name.split('/').any(|part| part == ".." || part == ".");

        if escapes {
            return Err(format!(
                "pagefind named a file `{name}`, which is not a path inside its own bundle"
            ));
        }

        Ok(format!("{UI_DIR}/{BUNDLE_DIR}/{name}"))
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        #[test]
        fn a_bundle_file_goes_under_the_ui_directory() {
            assert_eq!(
                bundle_path(Path::new("pagefind.js")),
                Ok("_/pagefind/pagefind.js".to_string()),
            );

            assert_eq!(
                bundle_path(Path::new("fragment/en_0000.pf_fragment")),
                Ok("_/pagefind/fragment/en_0000.pf_fragment".to_string()),
            );
        }

        #[test]
        fn a_file_that_would_be_written_outside_the_site_is_refused() {
            // Every one of these is joined onto the output directory, and none
            // of them lands in it.
            for name in ["/etc/passwd", "../outside.js", "", "./pagefind.js"] {
                assert!(
                    bundle_path(Path::new(name)).is_err(),
                    "`{name}` was allowed through"
                );
            }
        }

        #[test]
        fn the_interfaces_the_shell_does_not_use_are_left_out() {
            assert!(unused(Path::new("pagefind-ui.js")));
            assert!(unused(Path::new("pagefind-modular-ui.css")));

            // The index itself, and the module the shell reads it through.
            assert!(!unused(Path::new("pagefind.js")));
            assert!(!unused(Path::new("pagefind-entry.json")));
            assert!(!unused(Path::new("wasm.en.pagefind")));
            assert!(!unused(Path::new("fragment/en_0000.pf_fragment")));
        }
    }
}
