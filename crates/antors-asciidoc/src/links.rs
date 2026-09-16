//! Resolving `xref:` and image targets across the whole site.
//!
//! A reference in Antora may name a page in another module, another component
//! or another version, and what it becomes is a *relative* URL from the page
//! that wrote it to the page it names. Neither end knows where the other sits,
//! so both questions — which file is meant, and how to get there from here —
//! are answered by the catalog.
//!
//! The same type answers three of the parser's questions, because all three are
//! the same question asked about different macros: [`ReferenceResolver`] for
//! `xref:`, [`PathResolver`] for an image's `src`, and [`InlineRenderer`] for
//! the class an anchor carries.

use std::{
    cell::RefCell,
    rc::Rc,
    sync::Arc,
};

use antors_content::Catalog;
use antors_model::{
    Family,
    ResourceId,
    resource::{
        Context,
        Key,
    },
    url::relativize,
};
use asciidoc_parser::{
    document::Catalog as DocumentCatalog,
    parser::{
        CatalogResolver,
        DefaultPathResolver,
        HtmlInlineRenderer,
        InlineRenderer,
        PathResolver,
        ReferenceResolver,
        ResolutionContext,
        ResolvedReference,
        XrefRenderParams,
    },
};

/// Everything the parser needs to turn a target into a link, for one page.
#[derive(Debug)]
pub struct Links {
    /// Every resource in the site.
    ///
    /// Owned rather than borrowed because the parser holds its path resolver
    /// behind an `Rc` for the life of the parse and so requires a `'static`
    /// one; the catalog is shared, not copied.
    catalog: Arc<Catalog>,

    /// The document's own catalog, for a reference to something on this page.
    ///
    /// Filled in after the parse, because that is when the document has one —
    /// and it is a clone rather than a borrow because resolving takes the
    /// document mutably, and the document owns the catalog a resolver would
    /// borrow.
    document: RefCell<Option<DocumentCatalog>>,

    /// The page being rendered, which is what a reference is relative to.
    page: Key,

    /// Where that page is published, or `None` to leave every link as an
    /// absolute site path.
    ///
    /// A navigation file is not published anywhere and is rendered into every
    /// page of its component version, so its links cannot be made relative to
    /// any one of them — they are stored absolute and relativized per page.
    page_url: Option<String>,

    /// Targets that named nothing, so the build can report them.
    unresolved: RefCell<Vec<String>>,

    /// Asciidoctor's own path arithmetic, for a target that is a path rather
    /// than a resource ID.
    fallback: DefaultPathResolver,
}

impl Links {
    /// Build the resolver for one page, whose links are relative to it.
    pub fn new(catalog: Arc<Catalog>, page: Key, page_url: String) -> Self {
        Self {
            catalog,
            document: RefCell::new(None),
            page,
            page_url: Some(page_url),
            unresolved: RefCell::new(Vec::new()),
            fallback: DefaultPathResolver::default(),
        }
    }

    /// Build the resolver for a file that is not published, whose links stay
    /// absolute.
    pub fn absolute(catalog: Arc<Catalog>, page: Key) -> Self {
        Self {
            catalog,
            document: RefCell::new(None),
            page,
            page_url: None,
            unresolved: RefCell::new(Vec::new()),
            fallback: DefaultPathResolver::default(),
        }
    }

    /// The link to `url` as this file should write it.
    fn link_to(&self, url: &str) -> String {
        match &self.page_url {
            Some(from) => relativize(from, url),
            None => url.to_string(),
        }
    }

    /// Hand over the catalog the parse built, so that a reference to something
    /// on this page can be resolved too.
    pub fn adopt(&self, document: DocumentCatalog) {
        *self.document.borrow_mut() = Some(document);
    }

    /// Every target that named nothing.
    pub fn unresolved(&self) -> Vec<String> {
        self.unresolved.borrow().clone()
    }

    /// Resolve the media targets that never reached [`web_path`].
    ///
    /// # Why this exists
    ///
    /// `image::guide:screenshot.svg[]` names an image in another module, and
    /// resolving it is what [`PathResolver`] is for — but two things upstream
    /// keep the target from arriving there:
    ///
    /// - `adocers-html` joins `imagesdir` onto a *block* image, video or audio
    ///   target itself rather than routing it through the parser's path
    ///   resolver, so a host resolver never sees it.
    /// - The parser treats a target that looks like `scheme:rest` as a URI and
    ///   passes it through untouched, and `guide:screenshot.svg` looks exactly
    ///   like one.
    ///
    /// So the finished markup is searched for the targets that got away. This
    /// is a workaround and is shaped like one: it should be deleted the moment
    /// the block-media path consults the resolver, and nothing else in this
    /// crate depends on it.
    ///
    /// [`web_path`]: PathResolver::web_path
    pub fn resolve_media(&self, html: &str, images_dir: &str) -> String {
        let mut out = String::with_capacity(html.len());
        let mut rest = html;

        while let Some(start) = rest.find(" src=\"") {
            let open = start + " src=\"".len();

            let Some(end) = rest[open..].find('"').map(|index| index + open) else {
                break;
            };

            out.push_str(&rest[..open]);

            let target = &rest[open..end];
            out.push_str(&self.resolved_media_target(target, images_dir));

            rest = &rest[end..];
        }

        out.push_str(rest);

        out
    }

    /// One `src` value, resolved if it turns out to name a resource.
    fn resolved_media_target(&self, target: &str, images_dir: &str) -> String {
        // The block-media path has already prepended `imagesdir`, so it comes
        // off again before the remainder is read as a resource ID.
        let bare = images_dir
            .is_empty()
            .then_some(target)
            .or_else(|| target.strip_prefix(&format!("{images_dir}/")))
            .unwrap_or(target);

        self.resource(bare, Family::Image)
            .map_or_else(|| target.to_string(), |resolution| resolution.href)
    }

    /// The page's own coordinates, for filling in what a reference leaves out.
    fn context(&self) -> Context {
        Context {
            component: self.page.component.clone(),
            version: self.page.version.clone(),
            module: self.page.module.clone(),
        }
    }

    /// Resolve a target that names a resource, or `None` if it does not name
    /// one.
    fn resource(&self, target: &str, default_family: Family) -> Option<Resolution> {
        let (path, fragment) = split_fragment(target);

        if !names_a_resource(path, default_family) {
            return None;
        }

        let id = ResourceId::parse(path).ok()?;
        let resolved = id.resolve_in(&self.context(), default_family);
        let file = self.catalog.resolve(&resolved)?;
        let url = file.url()?;

        let href = match fragment {
            Some(fragment) => self.link_to(&format!("{url}#{fragment}")),
            None => self.link_to(url),
        };

        // A reference with a fragment cannot be named after the target page:
        // it points into the page, not at it, and the page's title would
        // describe the wrong thing. Antora shows the target as written, which
        // at least says where the reader is being sent.
        let text = if fragment.is_some() {
            target.to_string()
        } else {
            file.display_text()
        };

        Some(Resolution {
            href,
            text,
            family: file.key.family,
        })
    }
}

/// What a resolved reference became.
struct Resolution {
    /// The relative URL.
    href: String,

    /// What to show when the reference supplied no text.
    text: String,

    /// What was pointed at, which decides the anchor's class.
    family: Family,
}

/// Whether a target names a resource rather than something nearer to hand.
///
/// A resource ID is recognized by shape, and what counts depends on what the
/// macro would otherwise have meant:
///
/// - For a reference, the alternative is an anchor on this page —
///   `<<section-id>>`, or a natural reference like `<<The Later Section>>`.
///   Neither has an extension, so naming one is what marks a document.
/// - For an image, the alternative is a path under `imagesdir`, and a path may
///   have any extension at all. So the mark is a *prefix*: a `:` or an `@`
///   before the first `/`, which no file name has and every qualified resource
///   ID does.
fn names_a_resource(path: &str, default_family: Family) -> bool {
    if path.is_empty() || path.contains(char::is_whitespace) || path.contains("://") {
        return false;
    }

    // A family prefix is unambiguous whatever the macro was.
    if path.contains('$') {
        return true;
    }

    if default_family == Family::Page {
        // Case sensitive on purpose: a resource ID names its file exactly, so
        // a target written `Other.ADOC` names nothing.
        #[expect(
            clippy::case_sensitive_file_extension_comparisons,
            reason = "a resource ID names its file exactly; see above"
        )]
        return path.ends_with(".adoc");
    }

    let head = path.split('/').next().unwrap_or(path);

    head.contains(':') || head.contains('@')
}

/// Split `page.adoc#fragment` into its two halves.
fn split_fragment(target: &str) -> (&str, Option<&str>) {
    match target.split_once('#') {
        Some((path, fragment)) => (path, Some(fragment)),
        None => (target, None),
    }
}

impl ReferenceResolver for Links {
    fn resolve(&self, context: &ResolutionContext<'_>) -> Option<ResolvedReference> {
        if let Some(resolution) = self.resource(context.target, Family::Page) {
            return Some(ResolvedReference::new(
                resolution.href,
                Some(resolution.text),
            ));
        }

        // Not a resource, so it is a reference to something on this page, and
        // the document's own catalog is what knows about those.
        let document = self.document.borrow();

        let resolved = document
            .as_ref()
            .and_then(|catalog| CatalogResolver::new(catalog).resolve(context));

        if resolved.is_none() && context.derived.is_none() {
            self.unresolved
                .borrow_mut()
                .push(context.target.to_string());
        }

        resolved
    }
}

impl PathResolver for Links {
    fn web_path(&self, target: &str, start: Option<&str>) -> String {
        // An image may name a module or a component, exactly as a reference
        // may — `image::guide:screenshot.svg[]` is the module's image, not a
        // file in a directory called `guide`.
        if let Some(resolution) = self.resource(target, Family::Image) {
            return resolution.href;
        }

        // Anything else is a path under `imagesdir`, which is where an image
        // in this page's own module already is.
        self.fallback.web_path(target, start)
    }
}

impl InlineRenderer for Links {
    fn render_xref(&self, params: &XrefRenderParams<'_>, dest: &mut String) {
        let mut anchor = String::new();
        HtmlInlineRenderer {}.render_xref(params, &mut anchor);

        // The class says what kind of thing was linked to, which is what lets
        // a stylesheet mark an attachment differently from a page without the
        // author having done anything.
        let class = match self.resource(params.target, Family::Page) {
            Some(resolution) if resolution.family == Family::Attachment => "xref attachment",
            Some(_) => "xref page",
            None => return dest.push_str(&anchor),
        };

        dest.push_str(&with_class(&anchor, class));
    }
}

/// Add `class` to the first anchor in `anchor`, merging with one already there.
///
/// The renderer that produced the markup already knows how to write an anchor —
/// with its `target`, its `rel`, its roles and its text — and reproducing that
/// here to add one attribute would be two implementations of one thing,
/// drifting apart at the first change. So the markup is produced and then
/// adjusted.
fn with_class(anchor: &str, class: &str) -> String {
    let Some(start) = anchor.find("<a ") else {
        return anchor.to_string();
    };

    let open = start + "<a".len();

    // A `class="…"` the renderer already wrote holds the roles the author
    // asked for, and those come first: a stylesheet that reads `.xref` and one
    // that reads a role should both find what they expect.
    if let Some(existing) = anchor[open..].find("class=\"") {
        let at = open + existing + "class=\"".len();
        return format!("{}{class} {}", &anchor[..at], &anchor[at..]);
    }

    format!("{} class=\"{class}\"{}", &anchor[..open], &anchor[open..])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_document_target_names_a_resource() {
        assert!(names_a_resource("blocks.adoc", Family::Page));
        assert!(names_a_resource("guide:start.adoc", Family::Page));
        assert!(names_a_resource(
            "1.0@showcase:ROOT:legacy.adoc",
            Family::Page
        ));
        assert!(names_a_resource("attachment$report.pdf", Family::Page));
    }

    #[test]
    fn an_in_page_target_does_not() {
        assert!(!names_a_resource("section-id", Family::Page));
        assert!(!names_a_resource("The Later Section", Family::Page));
        assert!(!names_a_resource("", Family::Page));
    }

    #[test]
    fn a_qualified_image_target_names_a_resource() {
        // An image has no extension to go by, so the prefix is what marks it.
        assert!(names_a_resource("guide:screenshot.svg", Family::Image));
        assert!(names_a_resource("sidecar::logo.png", Family::Image));
        assert!(names_a_resource("1.0@diagram.svg", Family::Image));
        assert!(names_a_resource("image$logo.svg", Family::Image));
    }

    #[test]
    fn a_plain_image_path_is_a_path() {
        assert!(!names_a_resource("logo.svg", Family::Image));
        assert!(!names_a_resource("sub/dir/logo.svg", Family::Image));
        assert!(!names_a_resource(
            "https://example.org/a.png",
            Family::Image
        ));
    }

    #[test]
    fn a_fragment_comes_off_first() {
        assert_eq!(
            split_fragment("blocks.adoc#sidebar"),
            ("blocks.adoc", Some("sidebar"))
        );
        assert_eq!(split_fragment("blocks.adoc"), ("blocks.adoc", None));
    }

    #[test]
    fn a_class_is_added_to_a_bare_anchor() {
        assert_eq!(
            with_class(r#"<a href="a.html">A</a>"#, "xref page"),
            r#"<a class="xref page" href="a.html">A</a>"#
        );
    }

    #[test]
    fn a_class_joins_the_roles_already_there() {
        assert_eq!(
            with_class(r#"<a href="a.html" class="big">A</a>"#, "xref page"),
            r#"<a href="a.html" class="xref page big">A</a>"#
        );
    }

    #[test]
    fn markup_with_no_anchor_is_left_alone() {
        assert_eq!(with_class("[missing]", "xref page"), "[missing]");
    }
}

/// A [`Links`] several of the parser's seams can hold at once.
///
/// The parser takes ownership of its path resolver and keeps it behind an
/// `Rc`, but the same object also answers for references and renders anchors
/// *after* the parse — so the build has to keep a handle on it. This newtype is
/// the handle: the orphan rule stops `PathResolver` being implemented for
/// `Rc<Links>` directly.
#[derive(Clone, Debug)]
pub struct Shared(pub Rc<Links>);

impl PathResolver for Shared {
    fn web_path(&self, target: &str, start: Option<&str>) -> String {
        self.0.web_path(target, start)
    }
}
