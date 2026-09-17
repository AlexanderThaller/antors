//! Typesetting a page — or a whole component version — as a PDF.
//!
//! The PDF is [`adocers-typst`]'s, the same way the HTML is
//! [`adocers-html`]'s: this crate supplies Antora's seams and the back end does
//! the typesetting. So a page here and a standalone document rendered by
//! `adocers` are laid out by the same code, and there is no second renderer to
//! fall behind the first.
//!
//! # Two things a PDF has to be told that a page does not
//!
//! **Where the pictures are.** Typst has no file system, so the back end reads
//! every image off disk relative to a base directory. A page's `imagesdir`
//! points at a *URL* and is rewritten afterwards against the catalog, which a
//! PDF cannot do — so a PDF is parsed with `imagesdir` naming the module's
//! `_images` directory *in the output*, as an absolute path, and that same
//! directory is the base. That is why PDFs are written after the images are
//! copied and not before.
//!
//! Both, because the back end reads the two kinds of image differently: an
//! inline `image:` arrives having been through `imagesdir`, and a block
//! `image::` arrives as the author wrote it and is looked for under the base.
//! An image that names *another* module satisfies neither and is named in the
//! text rather than drawn — see [`Renderer::pdf`].
//!
//! **Where the site is.** A page's links are relative to the page, which works
//! because the page never moves. A PDF is downloaded and opened somewhere else
//! entirely, where a relative link points at nothing — so when the playbook
//! knows the site's address, every link in a PDF is written against it.
//!
//! [`adocers-typst`]: https://crates.io/crates/adocers-typst
//! [`adocers-html`]: https://crates.io/crates/adocers-html

use std::{
    fmt::Write as _,
    path::{
        Path,
        PathBuf,
    },
    sync::Arc,
};

use antors_content::{
    ComponentVersion,
    SourceFile,
};
use antors_model::{
    Family,
    resource::{
        Key,
        ROOT_MODULE,
    },
    url::{
        ExtensionStyle,
        locate,
    },
};

use crate::{
    attributes::{
        Attribute,
        Attributes,
    },
    links::Links,
    render::Renderer,
};

/// A typeset PDF, and anything worth telling the build about how it was made.
#[derive(Clone, Debug)]
pub struct Pdf {
    /// The PDF itself.
    pub bytes: Vec<u8>,

    /// What the back end gave up on, and what the source asked for and did not
    /// get.
    ///
    /// None of these stopped the PDF being made, so they are reported and the
    /// build goes on.
    pub notices: Vec<String>,
}

/// Why a PDF could not be made.
#[derive(Debug, thiserror::Error)]
pub enum PdfError {
    /// The source could not be read.
    #[error(transparent)]
    Read(#[from] std::io::Error),

    /// Typst would not lay the document out.
    #[error("{0}")]
    Typeset(String),

    /// A component version with no pages has no manual.
    #[error("there are no pages to typeset")]
    Empty,
}

impl Renderer {
    /// Typeset one page as a PDF.
    ///
    /// `output` is the build's output directory, which is where the images the
    /// page names are read from — so every image it shows has to have been
    /// copied there already.
    ///
    /// # What a PDF does not show
    ///
    /// An image from another module — `image::guide:screenshot.svg[]` — is
    /// named in the text rather than drawn. The back end looks a block image up
    /// by the path the author wrote, under one directory, and a resource ID is
    /// not a path in any directory.
    pub fn pdf(
        &self,
        page: &SourceFile,
        component_version: &ComponentVersion,
        output: &Path,
    ) -> Result<Pdf, PdfError> {
        let source = page.contents.read_to_string()?;

        let images = self.images_of(&page.key, output);

        let mut attributes =
            Attributes::for_page(self.playbook(), self.catalog(), component_version, page);

        attributes.set("imagesdir", Attribute::hard(path(&images)));

        let links = Links::new(
            Arc::clone(self.catalog()),
            page.key.clone(),
            page.url().unwrap_or_default().to_string(),
        )
        .away_from(self.playbook().site.url.clone());

        let (mut parser, wiring) = self.parser_with(
            page.key.clone(),
            &page.relative_src_path,
            &attributes,
            links,
        );

        let mut document = parser.parse_deferred(&source);

        wiring.links.adopt(document.catalog().clone());
        document.resolve_references(&*wiring.links, &*wiring.links, &parser);

        let mut notices: Vec<String> = wiring
            .includes
            .missing()
            .into_iter()
            .map(|target| format!("include target not found: {target}"))
            .collect();

        let pdf = adocers_typst::pdf(&document, &images, &self.typst_options())
            .map_err(|error| PdfError::Typeset(format!("{error:#}")))?;

        notices.extend(pdf.fallback);

        Ok(Pdf {
            bytes: pdf.bytes,
            notices,
        })
    }

    /// Typeset every page of one component version as a single PDF.
    ///
    /// `pages` is the order the pages appear in, which the caller takes from
    /// the navigation rather than from the file system — a manual reads in the
    /// order its author put its pages in.
    ///
    /// `url` is where the PDF itself is published, which is what the links
    /// inside it are relative to when the site does not know its own address.
    pub fn manual(
        &self,
        component_version: &ComponentVersion,
        pages: &[&SourceFile],
        url: &str,
        output: &Path,
    ) -> Result<Pdf, PdfError> {
        let Some(first) = pages.first() else {
            return Err(PdfError::Empty);
        };

        let descriptor = &component_version.descriptor;

        let key = Key {
            component: descriptor.name.clone(),
            version: descriptor.version(),
            module: ROOT_MODULE.to_string(),
            family: Family::Page,
            relative: String::new(),
        };

        // One directory for the whole document, because the back end has only
        // one: the root module's, which is where a site that keeps its images
        // in one place keeps them.
        let images = self.images_of(&key, output);

        let mut attributes =
            Attributes::for_page(self.playbook(), self.catalog(), component_version, first);

        // The pages being gathered come from modules whose images sit in
        // different directories, so the source below moves `imagesdir` before
        // every include — which it can only do if this is soft. A playbook is
        // free to hard-set the same attribute, and this overrules it: what it
        // would be overruling is a URL, in a document that reads files.
        attributes.set("imagesdir", Attribute::soft(path(&images)));

        // Likewise, and for a sharper reason: two pages that both have a
        // section called "Overview" generate the same ID twice, and Typst will
        // not lay out a document that points at a label defined more than once.
        // The source below gives each page a prefix of its own.
        attributes.set("idprefix", Attribute::soft("_"));

        // A page's own outline is unset, because the shell places it beside the
        // article. A manual has no shell and every reason to open with its own
        // contents.
        attributes.set("toc", Attribute::soft(""));
        attributes.set("toclevels", Attribute::soft("2"));

        let links = Links::new(Arc::clone(self.catalog()), key.clone(), url.to_string())
            .away_from(self.playbook().site.url.clone());

        let source = self.gather(component_version, pages, output);

        let (mut parser, wiring) = self.parser_with(
            key,
            &format!("{}.adoc", descriptor.name),
            &attributes,
            links,
        );

        let mut document = parser.parse_deferred(&source);

        wiring.links.adopt(document.catalog().clone());
        document.resolve_references(&*wiring.links, &*wiring.links, &parser);

        let mut notices: Vec<String> = wiring
            .includes
            .missing()
            .into_iter()
            .map(|target| format!("include target not found: {target}"))
            .collect();

        let pdf = adocers_typst::pdf(&document, &images, &self.typst_options())
            .map_err(|error| PdfError::Typeset(format!("{error:#}")))?;

        notices.extend(pdf.fallback);

        Ok(Pdf {
            bytes: pdf.bytes,
            notices,
        })
    }

    /// The `AsciiDoc` source of a whole component version.
    ///
    /// Every page is *included* rather than concatenated, so each one is read,
    /// resolved and offset by the parser exactly as it is when it stands
    /// alone — and a partial it includes still resolves against the page that
    /// included it rather than against this file.
    ///
    /// Two attributes move between the includes:
    ///
    /// - `imagesdir`, because the pages come from modules whose images sit in
    ///   different directories.
    /// - `idprefix`, because two pages that both have a section called
    ///   "Overview" would otherwise generate the same ID twice, and Typst will
    ///   not lay out a document that points at a label defined more than once.
    ///   A manual in which one cross reference fails is better than no manual.
    fn gather(
        &self,
        component_version: &ComponentVersion,
        pages: &[&SourceFile],
        output: &Path,
    ) -> String {
        let descriptor = &component_version.descriptor;

        let mut out = format!("= {}\n", descriptor.title());

        let version = descriptor.display_version();

        if !version.is_empty() {
            let _ = writeln!(out, ":revnumber: {version}");
        }

        out.push('\n');

        for (index, page) in pages.iter().enumerate() {
            let _ = writeln!(
                out,
                ":imagesdir: {}",
                path(&self.images_of(&page.key, output)),
            );

            // Distinct per page and a valid ID start, so the sections of one
            // page cannot collide with the sections of another.
            let _ = writeln!(out, ":idprefix: _p{index}_");

            let _ = writeln!(
                out,
                "\ninclude::{}:page${}[leveloffset=+1]\n",
                page.key.module, page.key.relative,
            );
        }

        out
    }

    /// Where one module's images are, as a directory on disk.
    ///
    /// Absolute, and for a reason beyond tidiness: the back end reads an inline
    /// image's path by joining it onto this directory, and joining an absolute
    /// path replaces the base outright. So `imagesdir` being absolute is what
    /// makes an inline image resolve to the same file as a block one, whatever
    /// the base happens to be.
    ///
    /// Where the files go is asked of the arithmetic that put them there,
    /// rather than assembled here, so the two cannot disagree.
    fn images_of(&self, key: &Key, output: &Path) -> PathBuf {
        let probe = Key {
            component: key.component.clone(),
            version: key.version.clone(),
            module: key.module.clone(),
            family: Family::Image,
            relative: PROBE.to_string(),
        };

        let under = locate(&probe, self.catalog().extension_style())
            .and_then(|location| {
                location
                    .out
                    .strip_suffix(&format!("/{PROBE}"))
                    .map(str::to_string)
            })
            .unwrap_or_default();

        let directory = output.join(under);

        // A relative output directory would work for reading — the process
        // stays in one place — but not for the join above, which is the whole
        // point of computing this.
        std::path::absolute(&directory).unwrap_or(directory)
    }

    /// What the back end should put in the PDF beyond the document's content.
    fn typst_options(&self) -> adocers_typst::Options {
        let options = self.options();

        adocers_typst::Options {
            icons: options.icons,
            highlight: options.highlight,
            mermaid: options.mermaid,
            math: options.math,
        }
    }
}

/// A file name that stands in for a real one, to ask where its directory is.
const PROBE: &str = "_";

/// A directory as an `AsciiDoc` attribute's value.
///
/// `AsciiDoc` paths are `/`-separated wherever they are written, so a path from
/// the file system is written the same way — which on the one platform where
/// that is not already true is what the parser expects to be given.
fn path(directory: &Path) -> String {
    directory.to_string_lossy().replace('\\', "/")
}

/// Where a page's PDF is written, given where its HTML is.
///
/// The output path and not the URL, because the URL of a page need not end in
/// `.html` — `urls.html_extension_style` may drop the extension or turn the
/// page into a directory, and neither of those says anything about what the
/// file beside it is called.
pub fn beside(out: &str) -> String {
    format!("{}.pdf", out.strip_suffix(".html").unwrap_or(out))
}

/// What a component version's manual is called.
///
/// Named after the component and the version rather than something generic,
/// because it is downloaded: a reader with three manuals in one directory has
/// to be able to tell them apart, and `documentation.pdf` three times over
/// cannot.
pub fn manual_name(component: &str, version: &str) -> String {
    let component = slug(component);

    if version.is_empty() {
        return format!("{component}.pdf");
    }

    format!("{component}-{}.pdf", slug(version))
}

/// A name that is safe to put in a path and in a `Content-Disposition`.
fn slug(name: &str) -> String {
    let mut out = String::with_capacity(name.len());

    for character in name.chars() {
        if character.is_ascii_alphanumeric() || character == '-' || character == '.' {
            out.push(character.to_ascii_lowercase());
        } else if !out.ends_with('-') {
            out.push('-');
        }
    }

    let trimmed = out.trim_matches('-');

    if trimmed.is_empty() {
        "documentation".to_string()
    } else {
        trimmed.to_string()
    }
}

/// Where a component version's root is, as a path from the output directory.
///
/// Empty for a component published at the site root, which is what a component
/// called `ROOT` with no version is.
pub fn component_version_root(component: &str, version: &str, style: ExtensionStyle) -> String {
    // `index.adoc` rather than any other name, because it is the one page
    // `html_extension_style: indexify` leaves alone: every other page becomes a
    // directory of its own, and the answer would come back one level too deep.
    let probe = Key {
        component: component.to_string(),
        version: version.to_string(),
        module: ROOT_MODULE.to_string(),
        family: Family::Page,
        relative: "index.adoc".to_string(),
    };

    locate(&probe, style)
        .map(|location| match location.out.rsplit_once('/') {
            Some((directory, _)) => directory.to_string(),
            None => String::new(),
        })
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_pages_pdf_sits_beside_its_html() {
        assert_eq!(beside("showcase/2.0/index.html"), "showcase/2.0/index.pdf");

        // Indexified, where the page is a directory and the file inside it is
        // what gets a sibling.
        assert_eq!(
            beside("showcase/2.0/page/index.html"),
            "showcase/2.0/page/index.pdf"
        );
    }

    #[test]
    fn a_manual_is_named_after_what_it_documents() {
        assert_eq!(manual_name("showcase", "2.0"), "showcase-2.0.pdf");
        assert_eq!(manual_name("sidecar", ""), "sidecar.pdf");
    }

    #[test]
    fn a_manuals_name_survives_being_put_in_a_path() {
        assert_eq!(
            manual_name("My Product", "v1.0 (beta)"),
            "my-product-v1.0-beta.pdf"
        );
        assert_eq!(manual_name("///", ""), "documentation.pdf");
    }

    #[test]
    fn a_component_versions_root_is_where_its_pages_sit() {
        assert_eq!(
            component_version_root("showcase", "2.0", ExtensionStyle::Default),
            "showcase/2.0"
        );

        assert_eq!(
            component_version_root("ROOT", "", ExtensionStyle::Default),
            ""
        );

        // Every style publishes the component version in the same place; only
        // the pages inside it are named differently.
        for style in [ExtensionStyle::Drop, ExtensionStyle::Indexify] {
            assert_eq!(
                component_version_root("showcase", "2.0", style),
                "showcase/2.0",
                "{style:?}"
            );
        }
    }
}
