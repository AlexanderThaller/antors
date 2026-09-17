//! Writing the site's PDFs.
//!
//! Two kinds: one for every page, and one for the whole of each component
//! version. The typesetting itself is [`antors_asciidoc::pdf`]'s; what is here
//! is the question of where each file goes and which of them the page shell is
//! allowed to offer.
//!
//! # Why this pass runs where it does
//!
//! Between the resources and the pages, and for a reason at each end.
//!
//! *After* the resources, because Typst has no file system: the back end reads
//! every picture off disk, so an image that has not been copied into the output
//! yet is an image the PDF does not have.
//!
//! *Before* the pages, because a page carries a button to its own PDF and a
//! button to its component version's, and neither should be drawn for a file
//! that is not there. A PDF that would not typeset is reported and the build
//! goes on — with the button for it left out rather than pointing at nothing.
//!
//! [`antors_asciidoc::pdf`]: https://docs.rs/antors-asciidoc

use std::collections::BTreeMap;

use antors_model::resource::Key;

/// Which PDFs a build wrote, and where they are.
///
/// Every path in here is an absolute site path, the same as a page's own URL,
/// and is made relative to whichever page is being drawn.
#[derive(Clone, Debug, Default)]
pub(crate) struct Pdfs {
    /// Where each page's own PDF is published.
    pages: BTreeMap<Key, String>,

    /// Where each component version's whole manual is published.
    manuals: BTreeMap<(String, String), String>,
}

impl Pdfs {
    /// Where `page`'s PDF is, if it has one.
    pub(crate) fn page(&self, page: &Key) -> Option<&str> {
        self.pages.get(page).map(String::as_str)
    }

    /// Where one component version's manual is, if it has one.
    pub(crate) fn manual(&self, component: &str, version: &str) -> Option<&str> {
        self.manuals
            .get(&(component.to_string(), version.to_string()))
            .map(String::as_str)
    }
}

#[cfg(feature = "pdf")]
mod typeset {
    use std::{
        collections::{
            BTreeSet,
            HashMap,
        },
        sync::Arc,
    };

    use antors_asciidoc::{
        Renderer,
        pdf::{
            beside,
            component_version_root,
            manual_name,
        },
    };
    use antors_content::{
        Catalog,
        SourceFile,
    };
    use antors_model::resource::Key;

    use crate::{
        build::{
            BuildError,
            Navigation,
        },
        pdf::Pdfs,
        report::Report,
        write::Writer,
    };

    /// Typeset and write every PDF the site should have.
    pub(crate) fn write(
        catalog: &Arc<Catalog>,
        navigation: &std::collections::BTreeMap<(String, String), Navigation>,
        renderer: &Renderer,
        writer: &mut Writer,
        report: &mut Report,
    ) -> Result<Pdfs, BuildError> {
        let output = writer.root().to_path_buf();
        let mut pdfs = Pdfs::default();

        for component in catalog.components() {
            for component_version in &component.versions {
                let version = component_version.version();
                let key = (component.name.clone(), version.clone());

                let pages = ordered(catalog, navigation.get(&key), &component.name, &version);

                // The pages that came out, which is what the manual is built
                // from. A page the typesetter refused is refused again inside
                // a document that contains it, and the whole manual with
                // it — so one page nobody can print is not allowed to cost
                // every page that can be.
                let mut typeset: Vec<&SourceFile> = Vec::new();

                for page in &pages {
                    let Some(location) = &page.location else {
                        continue;
                    };

                    let out = beside(&location.out);

                    match renderer.pdf(page, component_version, &output) {
                        Ok(pdf) => {
                            for notice in pdf.notices {
                                report.warn(&page.relative_src_path, None, notice);
                            }

                            writer.file(&out, &pdf.bytes)?;
                            pdfs.pages.insert(page.key.clone(), format!("/{out}"));
                            typeset.push(page);
                        }

                        Err(error) => report.error(
                            &page.relative_src_path,
                            format!("this page could not be typeset as a PDF: {error}"),
                        ),
                    }
                }

                // A manual missing a page it should have is a manual somebody
                // will read to the end without noticing. Each page that could
                // not be typeset is already reported against itself; this says
                // what that cost.
                let left_out = pages.len() - typeset.len();

                if left_out > 0 {
                    report.warn(
                        &component.name,
                        None,
                        format!(
                            "{left_out} page(s) could not be typeset and are missing from this \
                             version's PDF"
                        ),
                    );
                }

                // A component version of one page *is* that page: a second
                // file with the same content under another name is not a
                // manual, and a button offering it is a button that does
                // nothing new.
                if typeset.len() < 2 {
                    continue;
                }

                let root =
                    component_version_root(&component.name, &version, catalog.extension_style());

                let name = manual_name(&component.name, &version);

                let out = if root.is_empty() {
                    name
                } else {
                    format!("{root}/{name}")
                };

                let url = format!("/{out}");

                match renderer.manual(component_version, &typeset, &url, &output) {
                    Ok(pdf) => {
                        for notice in pdf.notices {
                            report.warn(&component.name, None, notice);
                        }

                        writer.file(&out, &pdf.bytes)?;
                        pdfs.manuals.insert(key, url);
                    }

                    Err(error) => report.error(
                        &component.name,
                        format!("this component version could not be typeset as a PDF: {error}"),
                    ),
                }
            }
        }

        Ok(pdfs)
    }

    /// Every page of one component version, in the order a manual should read
    /// them.
    ///
    /// The navigation first, because that is where the author said what order
    /// the pages go in. Then whatever is left, in the catalog's own order: a
    /// page that is in no `nav.adoc` is still a page of the component version,
    /// and leaving it out would make the manual quietly incomplete.
    fn ordered<'a>(
        catalog: &'a Catalog,
        navigation: Option<&Navigation>,
        component: &str,
        version: &str,
    ) -> Vec<&'a SourceFile> {
        let published: Vec<&SourceFile> = catalog
            .pages_of(component, version)
            .filter(|page| page.location.is_some())
            .collect();

        let by_url: HashMap<&str, &SourceFile> = published
            .iter()
            .filter_map(|page| page.url().map(|url| (url, *page)))
            .collect();

        let mut seen: BTreeSet<&Key> = BTreeSet::new();
        let mut pages: Vec<&SourceFile> = Vec::new();

        if let Some(navigation) = navigation {
            for item in navigation.order() {
                let Some(url) = item.url.as_deref() else {
                    continue;
                };

                if let Some(page) = by_url.get(url)
                    && seen.insert(&page.key)
                {
                    pages.push(page);
                }
            }
        }

        for page in published {
            if seen.insert(&page.key) {
                pages.push(page);
            }
        }

        pages
    }
}

#[cfg(feature = "pdf")]
pub(crate) use typeset::write;
