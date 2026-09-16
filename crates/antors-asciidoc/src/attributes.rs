//! The attributes a page is parsed with.
//!
//! Three sets are layered, and the order is the whole of the rule: the
//! playbook's apply to the site, the component descriptor's to one component
//! version, and the built-in ones say where *this* page sits. Later layers win,
//! except that a value ending in `@` is a default the page itself may still
//! overrule.
//!
//! The built-in layer is the interesting one. It is what makes
//! `image::logo.svg[]` find a file three directories away without the author
//! saying so — `imagesdir` is set to the path from this page to its module's
//! `_images` — and what lets a template read `page-component-version` without
//! the page having been told which version it is in.

use std::collections::BTreeMap;

use antors_content::{
    Catalog,
    ComponentVersion,
    SourceFile,
};
use antors_model::{
    descriptor::AttributeValue,
    playbook::Playbook,
};

/// One attribute, and whether the page may change it.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Attribute {
    /// The value, or `None` to unset the attribute.
    pub value: Option<String>,

    /// Whether the document may override it.
    pub soft: bool,
}

impl Attribute {
    /// A value the document may not override.
    pub fn hard(value: impl Into<String>) -> Self {
        Self {
            value: Some(value.into()),
            soft: false,
        }
    }

    /// A value the document may override.
    pub fn soft(value: impl Into<String>) -> Self {
        Self {
            value: Some(value.into()),
            soft: true,
        }
    }

    /// An attribute the document may set but that starts unset.
    pub fn unset() -> Self {
        Self {
            value: None,
            soft: true,
        }
    }
}

/// The attributes one page is parsed with.
#[derive(Clone, Debug, Default)]
pub struct Attributes(BTreeMap<String, Attribute>);

impl Attributes {
    /// Build the attribute set for `page`.
    pub fn for_page(
        playbook: &Playbook,
        catalog: &Catalog,
        component_version: &ComponentVersion,
        page: &SourceFile,
    ) -> Self {
        let mut attributes = Self::default();

        attributes.add_defaults();
        attributes.add_site(playbook);
        attributes.add_map(&playbook.asciidoc.attributes);
        attributes.add_map(&component_version.descriptor.asciidoc.attributes);
        attributes.add_page(catalog, component_version, page);

        attributes
    }

    /// The attributes Antora sets for every page, whatever the playbook says.
    ///
    /// These come first so the playbook can change any of them — `icons` and
    /// `source-highlighter` in particular are things a site routinely has an
    /// opinion about.
    fn add_defaults(&mut self) {
        // A reference to an attribute nobody set is left in the text as
        // written. Saying so is the only way the author finds out, since the
        // reader is the wrong person to notice `{product-nme}`.
        self.set("attribute-missing", Attribute::soft("warn"));

        // Embedding an image in the page defeats the point of publishing it to
        // a URL the browser can cache, and in a site build it would inline the
        // same logo into every page.
        self.set("data-uri", Attribute::unset());

        self.set("icons", Attribute::soft("font"));
        self.set("sectanchors", Attribute::soft(""));

        // An AsciiDoc file is what a page is written in, and what an
        // inter-document reference is written against.
        self.set("docfilesuffix", Attribute::hard(".adoc"));

        // The build is the site generator, and a page may want to say something
        // different when it is read through one than when it is not.
        self.set("env", Attribute::hard("site"));
        self.set("env-site", Attribute::hard(""));
        self.set("site-gen", Attribute::hard("antora"));
        self.set("site-gen-antora", Attribute::hard(""));

        // The outline is drawn by the page shell, beside the article rather
        // than inside it, so a document that asks for one inline would get two.
        self.set("toc", Attribute::unset());
    }

    /// What the playbook's `site:` key contributes.
    fn add_site(&mut self, playbook: &Playbook) {
        if let Some(title) = &playbook.site.title {
            self.set("site-title", Attribute::hard(title.clone()));
        }

        if let Some(url) = &playbook.site.url {
            self.set("site-url", Attribute::hard(url.clone()));
        }
    }

    /// Apply an `asciidoc.attributes:` map from a playbook or a descriptor.
    fn add_map(&mut self, map: &BTreeMap<String, AttributeValue>) {
        for (name, value) in map {
            self.set(
                name,
                Attribute {
                    value: value.text(),
                    soft: value.is_soft(),
                },
            );
        }
    }

    /// The attributes that say where this page sits.
    ///
    /// Every one of these is hard-set: a page that could rewrite
    /// `page-component-version` could make the UI lie about where the reader
    /// is, and `imagesdir` is computed from the page's own URL — a page that
    /// changed it would only be breaking its own images.
    fn add_page(
        &mut self,
        catalog: &Catalog,
        component_version: &ComponentVersion,
        page: &SourceFile,
    ) {
        let descriptor = &component_version.descriptor;

        let module_root = page
            .location
            .as_ref()
            .map(|location| location.module_root_path.clone())
            .unwrap_or_default();

        self.set("imagesdir", Attribute::hard(under(&module_root, "_images")));
        self.set(
            "attachmentsdir",
            Attribute::hard(under(&module_root, "_attachments")),
        );

        // These two are resource-ID prefixes rather than directories, which is
        // Antora's way of making `include::{partialsdir}/x.adoc[]` — written
        // before families existed — keep working.
        self.set("partialsdir", Attribute::hard("partial$"));
        self.set("examplesdir", Attribute::hard("example$"));

        self.set(
            "docname",
            Attribute::hard(
                page.key
                    .relative
                    .strip_suffix(".adoc")
                    .unwrap_or(&page.key.relative),
            ),
        );

        self.set("docfile", Attribute::hard(page.relative_src_path.clone()));

        self.set(
            "page-component-name",
            Attribute::hard(descriptor.name.clone()),
        );
        self.set("page-component-title", Attribute::hard(descriptor.title()));
        self.set(
            "page-component-version",
            Attribute::hard(descriptor.version()),
        );
        self.set(
            "page-component-display-version",
            Attribute::hard(descriptor.display_version()),
        );
        self.set("page-version", Attribute::hard(descriptor.version()));
        self.set(
            "page-display-version",
            Attribute::hard(descriptor.display_version()),
        );
        self.set("page-module", Attribute::hard(page.key.module.clone()));
        // Relative to the *family* rather than to the component version, which
        // is what makes it the same string as the resource ID's path — the
        // name an author would use to refer to this page.
        self.set(
            "page-relative-src-path",
            Attribute::hard(page.key.relative.clone()),
        );

        if let Some(latest) = catalog
            .component(&descriptor.name)
            .and_then(antors_content::Component::latest)
            && latest.version() == descriptor.version()
        {
            // A page can use this to say "this is the current version" without
            // knowing what the current version is.
            self.set("page-component-latest-version", Attribute::hard(""));
        }

        self.add_origin(page);
    }

    /// Where the page's file came from.
    fn add_origin(&mut self, page: &SourceFile) {
        let origin = &page.origin;

        self.set("page-origin-type", Attribute::hard("git"));
        self.set(
            "page-origin-refname",
            Attribute::hard(origin.refname.clone()),
        );
        self.set(
            "page-origin-reftype",
            Attribute::hard(origin.reftype.as_str()),
        );
        self.set(
            format!("page-origin-{}", origin.reftype.as_str()),
            Attribute::hard(origin.refname.clone()),
        );
        self.set(
            "page-origin-start-path",
            Attribute::hard(origin.start_path.clone()),
        );

        if let Some(url) = &origin.url {
            self.set("page-origin-url", Attribute::hard(url.clone()));
        }

        if let Some(web_url) = &origin.web_url {
            self.set("page-origin-web-url", Attribute::hard(web_url.clone()));
        }

        if let Some(worktree) = &origin.worktree {
            self.set(
                "page-origin-worktree",
                Attribute::hard(worktree.to_string_lossy().into_owned()),
            );
        }
    }

    /// Set one attribute, replacing whatever an earlier layer said.
    pub fn set(&mut self, name: impl Into<String>, attribute: Attribute) {
        self.0.insert(name.into(), attribute);
    }

    /// Every attribute, in name order.
    pub fn iter(&self) -> impl Iterator<Item = (&str, &Attribute)> {
        self.0.iter().map(|(name, value)| (name.as_str(), value))
    }

    /// One attribute's value, if it is set.
    pub fn get(&self, name: &str) -> Option<&str> {
        self.0.get(name)?.value.as_deref()
    }
}

/// A path under the page's module root, with no leading `/` when the page is
/// already at that root.
fn under(module_root: &str, directory: &str) -> String {
    if module_root.is_empty() {
        return directory.to_string();
    }

    format!("{module_root}/{directory}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_page_at_its_module_root_has_a_bare_images_directory() {
        assert_eq!(under("", "_images"), "_images");
    }

    #[test]
    fn a_page_below_it_climbs_first() {
        assert_eq!(under("../..", "_images"), "../../_images");
    }
}
