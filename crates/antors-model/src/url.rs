//! Turning a resource ID into a path in the output and a URL in a page.
//!
//! A site's URLs are not stored anywhere; they are computed from the same five
//! parts a [resource ID] is made of, which is why a reference and its target
//! agree without either being told about the other. The arithmetic is small but
//! every part of it is load-bearing, so it lives in one place with the rules
//! written down.
//!
//! [resource ID]: crate::resource::ResourceId

use crate::resource::{
    Family,
    Key,
    ROOT_MODULE,
};

/// A component named this is published at the root of the site, with no
/// component segment of its own.
///
/// It is the escape hatch for a site that is one manual rather than a family of
/// them: without it every URL would begin with a component name the reader has
/// no use for.
pub const ROOT_COMPONENT: &str = "ROOT";

/// How a page's `.adoc` becomes a URL.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum ExtensionStyle {
    /// `page.adoc` is published at `page.html` and linked as `page.html`.
    #[default]
    Default,

    /// `page.adoc` is published at `page.html` and linked as `page` — for a
    /// server that adds the extension back.
    Drop,

    /// `page.adoc` is published at `page/index.html` and linked as `page/`,
    /// which needs no server configuration to give every page a clean URL.
    Indexify,
}

/// Where a resource is written, and how it is linked to.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Location {
    /// The path within the output directory, with `/` separators and no
    /// leading slash.
    pub out: String,

    /// The absolute site path a page links to, beginning with `/`.
    pub url: String,

    /// The relative path from this resource's directory to its module's root,
    /// which is what `imagesdir` and `attachmentsdir` are built from. Empty
    /// when the resource sits at the module root.
    pub module_root_path: String,

    /// The relative path from this resource's directory to the site root.
    pub root_path: String,
}

/// Work out where `key` is published.
///
/// A family that is not published has no location; [`Family::is_published`]
/// says which, and calling this for one of the others is a caller error rather
/// than something to represent.
pub fn locate(key: &Key, style: ExtensionStyle) -> Option<Location> {
    if !key.family.is_published() {
        return None;
    }

    let mut segments: Vec<String> = Vec::new();

    if key.component != ROOT_COMPONENT {
        segments.push(key.component.clone());
    }

    if !key.version.is_empty() {
        segments.push(key.version.clone());
    }

    // Everything above is the component version's root; everything below is
    // relative to it, and that boundary is what `module_root_path` measures
    // from.
    let module_root_depth = segments.len();

    if key.module != ROOT_MODULE {
        segments.push(key.module.clone());
    }

    if let Some(directory) = key.family.output_directory() {
        segments.push(directory.to_string());
    }

    let (out_tail, url_tail) = match key.family {
        Family::Page | Family::Alias => page_tail(&key.relative, style),
        _ => (key.relative.clone(), key.relative.clone()),
    };

    let out = join(&segments, &out_tail);
    let url = format!("/{}", join(&segments, &url_tail));

    // The depth a relative path has to climb is the number of *directory*
    // segments above it, so the file name at the end does not count.
    let depth = url.matches('/').count() - 1;

    let module_depth = depth.saturating_sub(module_root_depth);

    Some(Location {
        out,
        url,
        module_root_path: climb(module_depth),
        root_path: climb(depth),
    })
}

/// The output path and URL path for a page's own relative path.
fn page_tail(relative: &str, style: ExtensionStyle) -> (String, String) {
    let stem = relative.strip_suffix(".adoc").unwrap_or(relative);

    match style {
        ExtensionStyle::Default => (format!("{stem}.html"), format!("{stem}.html")),
        ExtensionStyle::Drop => (format!("{stem}.html"), stem.to_string()),

        ExtensionStyle::Indexify => {
            // `index.adoc` is already the name a directory URL resolves to, so
            // indexifying it again would bury it a directory deeper for no
            // gain.
            if stem == "index" || stem.ends_with("/index") {
                (format!("{stem}.html"), format!("{stem}.html"))
            } else {
                (format!("{stem}/index.html"), format!("{stem}/"))
            }
        }
    }
}

/// Join directory segments and a tail into a `/`-separated path.
fn join(segments: &[String], tail: &str) -> String {
    if segments.is_empty() {
        return tail.to_string();
    }

    format!("{}/{tail}", segments.join("/"))
}

/// `depth` levels of `..`, or the empty string at depth zero.
fn climb(depth: usize) -> String {
    if depth == 0 {
        return String::new();
    }

    let mut path = String::with_capacity(depth * 3 - 1);

    for level in 0..depth {
        if level > 0 {
            path.push('/');
        }

        path.push_str("..");
    }

    path
}

/// The URL `to`, written relative to a page published at `from`.
///
/// Both are absolute site paths. A target that is already a URL of its own — an
/// external link — is returned untouched, because there is nothing to make it
/// relative to.
pub fn relativize(from: &str, to: &str) -> String {
    if to.starts_with("http://") || to.starts_with("https://") || to.starts_with('#') {
        return to.to_string();
    }

    if !to.starts_with('/') || !from.starts_with('/') {
        return to.to_string();
    }

    // A fragment travels with the path but takes no part in the arithmetic.
    let (to_path, fragment) = match to.split_once('#') {
        Some((path, fragment)) => (path, Some(fragment)),
        None => (to, None),
    };

    let from_dirs: Vec<&str> = directories(from);
    let to_dirs: Vec<&str> = directories(to_path);
    let base = to_path.rsplit('/').next().unwrap_or_default();

    let shared = from_dirs
        .iter()
        .zip(&to_dirs)
        .take_while(|(a, b)| a == b)
        .count();

    let mut path = String::new();

    for _ in shared..from_dirs.len() {
        path.push_str("../");
    }

    for segment in &to_dirs[shared..] {
        path.push_str(segment);
        path.push('/');
    }

    path.push_str(base);

    // A link from a page to its own directory has nothing left to say; `./` is
    // the shortest thing a browser reads as "here".
    if path.is_empty() {
        path.push_str("./");
    }

    match fragment {
        Some(fragment) => format!("{path}#{fragment}"),
        None => path,
    }
}

/// The directory segments of an absolute site path, without the file name.
fn directories(path: &str) -> Vec<&str> {
    let without_base = match path.rfind('/') {
        Some(index) => &path[..index],
        None => path,
    };

    without_base.split('/').filter(|s| !s.is_empty()).collect()
}

#[cfg(test)]
mod tests {
    #![expect(clippy::unwrap_used, reason = "an unlocatable key is the test failing")]

    use super::*;

    fn key(component: &str, version: &str, module: &str, family: Family, relative: &str) -> Key {
        Key {
            component: component.to_string(),
            version: version.to_string(),
            module: module.to_string(),
            family,
            relative: relative.to_string(),
        }
    }

    #[test]
    fn a_root_module_page_sits_at_the_component_version_root() {
        let location = locate(
            &key("showcase", "2.0", "ROOT", Family::Page, "index.adoc"),
            ExtensionStyle::Default,
        )
        .unwrap();

        assert_eq!(location.out, "showcase/2.0/index.html");
        assert_eq!(location.url, "/showcase/2.0/index.html");
        assert_eq!(location.module_root_path, "");
        assert_eq!(location.root_path, "../..");
    }

    #[test]
    fn a_named_module_adds_a_segment() {
        let location = locate(
            &key("showcase", "2.0", "guide", Family::Page, "start.adoc"),
            ExtensionStyle::Default,
        )
        .unwrap();

        assert_eq!(location.url, "/showcase/2.0/guide/start.html");
        assert_eq!(location.module_root_path, "..");
        assert_eq!(location.root_path, "../../..");
    }

    #[test]
    fn a_page_in_a_subdirectory_climbs_out_of_it() {
        let location = locate(
            &key(
                "showcase",
                "2.0",
                "ROOT",
                Family::Page,
                "deep/nested/page.adoc",
            ),
            ExtensionStyle::Default,
        )
        .unwrap();

        assert_eq!(location.url, "/showcase/2.0/deep/nested/page.html");
        assert_eq!(location.module_root_path, "../..");
    }

    #[test]
    fn an_unversioned_component_has_no_version_segment() {
        let location = locate(
            &key("sidecar", "", "ROOT", Family::Page, "index.adoc"),
            ExtensionStyle::Default,
        )
        .unwrap();

        assert_eq!(location.url, "/sidecar/index.html");
        assert_eq!(location.root_path, "..");
    }

    #[test]
    fn the_root_component_has_no_component_segment() {
        let location = locate(
            &key("ROOT", "", "ROOT", Family::Page, "index.adoc"),
            ExtensionStyle::Default,
        )
        .unwrap();

        assert_eq!(location.url, "/index.html");
        assert_eq!(location.root_path, "");
    }

    #[test]
    fn images_and_attachments_go_under_their_own_directories() {
        let image = locate(
            &key("showcase", "2.0", "guide", Family::Image, "shot.svg"),
            ExtensionStyle::Default,
        )
        .unwrap();

        assert_eq!(image.url, "/showcase/2.0/guide/_images/shot.svg");

        let attachment = locate(
            &key("showcase", "2.0", "ROOT", Family::Attachment, "report.pdf"),
            ExtensionStyle::Default,
        )
        .unwrap();

        assert_eq!(attachment.url, "/showcase/2.0/_attachments/report.pdf");
    }

    #[test]
    fn a_partial_is_not_published() {
        assert!(
            locate(
                &key("showcase", "2.0", "ROOT", Family::Partial, "shared.adoc"),
                ExtensionStyle::Default,
            )
            .is_none()
        );
    }

    #[test]
    fn indexify_gives_a_page_a_directory_of_its_own() {
        let location = locate(
            &key("showcase", "2.0", "ROOT", Family::Page, "guide.adoc"),
            ExtensionStyle::Indexify,
        )
        .unwrap();

        assert_eq!(location.out, "showcase/2.0/guide/index.html");
        assert_eq!(location.url, "/showcase/2.0/guide/");
        assert_eq!(location.root_path, "../../..");
    }

    #[test]
    fn indexify_leaves_an_index_page_where_it_is() {
        let location = locate(
            &key("showcase", "2.0", "ROOT", Family::Page, "index.adoc"),
            ExtensionStyle::Indexify,
        )
        .unwrap();

        assert_eq!(location.out, "showcase/2.0/index.html");
    }

    #[test]
    fn relativize_within_a_directory_is_a_bare_name() {
        assert_eq!(
            relativize("/showcase/2.0/index.html", "/showcase/2.0/blocks.html"),
            "blocks.html"
        );
    }

    #[test]
    fn relativize_descends() {
        assert_eq!(
            relativize("/showcase/2.0/index.html", "/showcase/2.0/guide/start.html"),
            "guide/start.html"
        );
    }

    #[test]
    fn relativize_climbs() {
        assert_eq!(
            relativize("/showcase/2.0/index.html", "/showcase/1.0/legacy.html"),
            "../1.0/legacy.html"
        );

        assert_eq!(
            relativize("/showcase/2.0/index.html", "/sidecar/index.html"),
            "../../sidecar/index.html"
        );
    }

    #[test]
    fn relativize_keeps_a_fragment() {
        assert_eq!(
            relativize("/a/b/one.html", "/a/b/two.html#here"),
            "two.html#here"
        );
    }

    #[test]
    fn relativize_leaves_an_external_url_alone() {
        assert_eq!(
            relativize("/a/b/one.html", "https://example.org"),
            "https://example.org"
        );
    }

    #[test]
    fn relativize_to_a_directory_url() {
        assert_eq!(relativize("/a/b/one.html", "/a/b/"), "./");
        assert_eq!(relativize("/a/b/one.html", "/a/c/"), "../c/");
    }
}
