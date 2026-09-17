//! Parsing and rendering one page, with every Antora seam wired in.

use std::{
    collections::BTreeMap,
    path::PathBuf,
    rc::Rc,
    sync::Arc,
};

use adocers_html::Options as HtmlOptions;
use antors_content::{
    Catalog,
    ComponentVersion,
    SourceFile,
};
use antors_model::playbook::Playbook;
use asciidoc_parser::{
    Document,
    Parser,
    SafeMode,
    parser::ModificationContext,
    warnings::WarningSeverity,
};

use crate::{
    attributes::Attributes,
    details::{
        self,
        Detail,
    },
    include,
    links::{
        Links,
        Shared,
    },
};

/// What the renderer should do with the parts of a page it can leave out.
#[derive(Clone, Copy, Debug)]
#[expect(
    clippy::struct_excessive_bools,
    reason = "these mirror command line flags, and one field per flag is what reads clearly"
)]
pub struct Options {
    /// Whether an admonition is marked with an icon.
    pub icons: bool,

    /// Whether a source block is highlighted while the site is built, rather
    /// than in the reader's browser.
    pub highlight: bool,

    /// Whether a mermaid block is drawn as a diagram.
    pub mermaid: bool,

    /// Whether an equation is converted to `MathML`.
    pub math: bool,

    /// How deep the page's outline goes, unless the page says otherwise with
    /// `page-toclevels`.
    pub toc_levels: usize,
}

impl Default for Options {
    fn default() -> Self {
        Self {
            icons: true,
            highlight: true,
            mermaid: true,
            math: true,

            // Two levels is what the default Antora UI shows, and is about as
            // much as fits beside an article without becoming a second
            // article.
            toc_levels: 2,
        }
    }
}

/// What a page says about itself before it is rendered.
///
/// This is the result of the first of the build's two passes. It exists
/// because a reference with no text of its own shows the *target's* title, so
/// every title has to be known before any reference is resolved.
#[derive(Clone, Debug, Default)]
pub struct Header {
    /// The document title.
    pub title: Option<String>,

    /// What the navigation should show instead, from `:navtitle:`.
    pub nav_title: Option<String>,

    /// The resource IDs this page also answers to, from `:page-aliases:`.
    pub aliases: Vec<String>,

    /// What the page tagged itself with, from `:page-tags:`.
    ///
    /// Collected in the first pass because the tags overview is a page like any
    /// other and has to exist before anything renders — including before
    /// anything resolves a reference to it.
    pub tags: Vec<String>,
}

/// A rendered page.
#[derive(Clone, Debug, Default)]
pub struct Rendered {
    /// The article's markup: no page furniture, no title, no outline.
    pub html: String,

    /// The document title.
    pub title: Option<String>,

    /// The facts the document states about itself, for the shell to show under
    /// the title.
    pub details: Vec<Detail>,

    /// Every `page-*` attribute the page ended up with, for the templates.
    pub page_attributes: BTreeMap<String, String>,

    /// `:description:`, for the page's `<meta>`.
    pub description: Option<String>,

    /// `:keywords:`, likewise.
    pub keywords: Option<String>,

    /// The outline, as the back end rendered it, for the shell to place.
    ///
    /// `None` when the page has no sections to list.
    pub toc: Option<String>,

    /// Every file the parse read, for a watching build to watch.
    pub dependencies: Vec<PathBuf>,

    /// What went wrong that did not stop the render.
    pub warnings: Vec<Warning>,
}

/// Something the build should tell somebody about.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Warning {
    /// An `include::` named nothing.
    MissingInclude {
        /// The target, as written.
        target: String,
    },

    /// An `xref:` named nothing.
    UnresolvedReference {
        /// The target, as written.
        target: String,
    },

    /// The parser had something to say about the source.
    Parse {
        /// What it said.
        message: String,

        /// The line it said it about.
        line: usize,
    },
}

impl std::fmt::Display for Warning {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::MissingInclude { target } => write!(f, "include target not found: {target}"),

            Self::UnresolvedReference { target } => {
                write!(f, "reference target not found: {target}")
            }

            Self::Parse { message, line } => write!(f, "line {line}: {message}"),
        }
    }
}

/// Renders the pages of one site.
#[derive(Clone, Debug)]
pub struct Renderer {
    /// Every resource in the site.
    catalog: Arc<Catalog>,

    /// The build's configuration.
    playbook: Playbook,

    /// What to leave out.
    options: Options,
}

impl Renderer {
    /// A renderer for one site.
    pub fn new(catalog: Arc<Catalog>, playbook: Playbook, options: Options) -> Self {
        Self {
            catalog,
            playbook,
            options,
        }
    }

    /// Parse one page far enough to learn what it is called.
    ///
    /// This is the build's first pass. It parses the whole page rather than
    /// only its header, because `:navtitle:` and `:page-aliases:` may be set
    /// anywhere a document attribute may be, and because a title assembled from
    /// an attribute reference needs the attributes resolved to read it.
    pub fn header_of(&self, page: &SourceFile, component_version: &ComponentVersion) -> Header {
        let Ok(source) = page.contents.read_to_string() else {
            return Header::default();
        };

        let (mut parser, _links) = self.parser_for(page, component_version);
        let document = parser.parse_deferred(&source);

        Header {
            title: document.doctitle().map(str::to_string),
            nav_title: attribute(&document, "navtitle"),
            aliases: attribute(&document, "page-aliases")
                .map(|list| {
                    list.split(',')
                        .map(str::trim)
                        .filter(|alias| !alias.is_empty())
                        .map(str::to_string)
                        .collect()
                })
                .unwrap_or_default(),

            tags: details::of(&document)
                .into_iter()
                .find(|detail| detail.name == "tags")
                .map(|detail| detail.values.into_iter().map(|value| value.text).collect())
                .unwrap_or_default(),
        }
    }

    /// Render one page.
    pub fn render(
        &self,
        page: &SourceFile,
        component_version: &ComponentVersion,
    ) -> std::io::Result<Rendered> {
        let source = page.contents.read_to_string()?;

        let (mut parser, wiring) = self.parser_for(page, component_version);
        let mut document = parser.parse_deferred(&source);

        // The parse has just built the catalog of this page's own anchors, and
        // a reference to one of them resolves against it rather than against
        // the site.
        wiring.links.adopt(document.catalog().clone());

        document.resolve_references(&*wiring.links, &*wiring.links, &parser);

        let attributes =
            Attributes::for_page(&self.playbook, &self.catalog, component_version, page);

        let rendered = adocers_html::render(
            &document,
            &HtmlOptions {
                fragment: true,
                stylesheet: None,
                body_suffix: String::new(),
                icons: self.options.icons,
                highlight: self.options.highlight,
                mermaid: self.options.mermaid,
                math: self.options.math,

                // A site's outline depth is the playbook's to set, and a
                // page's `page-toclevels` overrides it — a name the back end
                // has no reason to know, so the answer is passed rather than
                // the question.
                toc_levels: Some(
                    attribute(&document, "page-toclevels")
                        .and_then(|levels| levels.parse().ok())
                        .unwrap_or(self.options.toc_levels),
                ),

                // The back end adds both of these to a *page* with a script,
                // and a page is not what it is being asked for. The site shell
                // supplies its own copy button and its own reading mark, over
                // markup it also renders — the navigation and the outline — so
                // one script covers both rather than three covering parts.
                copy: false,
                mark_reading: false,
            },
        );

        let html = wiring.links.resolve_media(
            &rendered.html,
            attributes.get("imagesdir").unwrap_or_default(),
        );

        let mut warnings: Vec<Warning> = document
            .warnings()
            // The parser marks its lowest-severity diagnostics `Debug` and says
            // a host is expected to suppress them: they report something a tool
            // might want without suggesting the parse is wrong. The one that
            // matters here is a block style the parser does not know — which a
            // back end may well know, and does: a `[mermaid]` listing is drawn
            // as a diagram, and warning about it on every page would bury
            // everything worth reading.
            .filter(|warning| warning.severity == WarningSeverity::Warning)
            .map(|warning| Warning::Parse {
                message: warning.warning.to_string(),
                line: document.origin_of(warning.source).line,
            })
            .collect();

        warnings.extend(
            wiring
                .includes
                .missing()
                .into_iter()
                .map(|target| Warning::MissingInclude { target }),
        );

        // An unresolved reference is not reported here: the parser has already
        // recorded one for every target it could not resolve, with the line it
        // was written on, and the same problem twice in a build log is one
        // report the reader has to work out is a duplicate.

        Ok(Rendered {
            html,
            title: document.doctitle().map(str::to_string),
            details: details::of(&document),
            page_attributes: page_attributes(&document),
            description: attribute(&document, "description"),
            keywords: attribute(&document, "keywords"),
            toc: rendered.toc,
            dependencies: wiring.includes.read(),
            warnings,
        })
    }

    /// Build a parser for one page, with every seam wired in.
    fn parser_for(
        &self,
        page: &SourceFile,
        component_version: &ComponentVersion,
    ) -> (Parser, Wiring) {
        let includes = Rc::new(include::Resolver::new(
            Arc::clone(&self.catalog),
            page.key.clone(),
        ));

        let links = Rc::new(Links::new(
            Arc::clone(&self.catalog),
            page.key.clone(),
            page.url().unwrap_or_default().to_string(),
        ));

        let mut parser = Parser::default()
            // A site build reads files the author named and nothing else. The
            // include handler enforces that by only resolving what is in the
            // catalog, so the safe mode is belt to its braces.
            .with_safe_mode(SafeMode::Safe)
            .with_primary_file_name(&page.relative_src_path)
            .with_include_file_handler(Rc::clone(&includes))
            .with_path_resolver(Shared(Rc::clone(&links)));

        for (name, attribute) in
            Attributes::for_page(&self.playbook, &self.catalog, component_version, page).iter()
        {
            let context = if attribute.soft {
                ModificationContext::Anywhere
            } else {
                ModificationContext::ApiOnly
            };

            parser = match &attribute.value {
                Some(value) => parser.with_intrinsic_attribute(name, value, context),
                None => parser.with_intrinsic_attribute_bool(name, false, context),
            };
        }

        (parser, Wiring { includes, links })
    }
}

/// The handles a parse leaves behind, for reading its results afterwards.
struct Wiring {
    /// What the include directives resolved to.
    includes: Rc<include::Resolver>,

    /// What the references resolved to.
    links: Rc<Links>,
}

/// Parse one page's header, without a site around it.
///
/// This exists for callers that have a file and no catalog — a preview of a
/// single page, a test — and takes the shortest path to a title.
pub fn title_of(source: &str) -> Option<String> {
    Parser::default()
        .parse_deferred(source)
        .doctitle()
        .map(str::to_string)
}

/// One document attribute's value, if it is set to anything.
fn attribute(document: &Document<'_>, name: &str) -> Option<String> {
    match document.attribute_value(name) {
        asciidoc_parser::document::InterpretedValue::Value(value) => Some(value.clone()),
        asciidoc_parser::document::InterpretedValue::Set => Some(String::new()),
        asciidoc_parser::document::InterpretedValue::Unset => None,
    }
}

/// Every `page-*` attribute the document ended up with.
///
/// The namespace is Antora's contract with the UI: anything in it is page
/// metadata a template may read, and anything outside it is an instruction to
/// the renderer that a template has no business acting on.
fn page_attributes(document: &Document<'_>) -> BTreeMap<String, String> {
    let mut attributes = BTreeMap::new();

    for attribute in document.header().attributes() {
        let name = attribute.name().data();

        let Some(short) = name.strip_prefix("page-") else {
            continue;
        };

        if let asciidoc_parser::document::InterpretedValue::Value(value) = attribute.value() {
            attributes.insert(short.to_string(), value.clone());
        }
    }

    attributes
}
