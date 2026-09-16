//! The sitemaps.
//!
//! A site that knows its own address can tell a search engine what it
//! contains. One sitemap per component keeps each under the 50,000-URL limit
//! without anyone having to think about it, and the index at the root points
//! at them.

use std::fmt::Write as _;

use antors_content::{
    Catalog,
    SourceFile,
};
use antors_model::{
    Family,
    Playbook,
};

/// Every sitemap the site should carry, as `(path, contents)`.
///
/// A site with no `site.url` carries none: a sitemap is a list of absolute
/// URLs, and there is no way to write one without knowing where the site
/// lives.
pub(crate) fn sitemaps(playbook: &Playbook, catalog: &Catalog) -> Vec<(String, String)> {
    let Some(base) = playbook
        .site
        .url
        .as_deref()
        .map(|url| url.trim_end_matches('/'))
    else {
        return Vec::new();
    };

    let mut sitemaps = Vec::new();
    let mut index = String::from(
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<sitemapindex \
         xmlns=\"http://www.sitemaps.org/schemas/sitemap/0.9\">\n",
    );

    for component in catalog.components() {
        let urls: Vec<String> = component
            .versions
            .iter()
            .flat_map(|version| {
                let version = version.version();

                catalog
                    .pages_of(&component.name, &version)
                    .filter(|page| page.key.family == Family::Page)
                    .filter_map(SourceFile::url)
                    .map(|url| format!("{base}{url}"))
                    .collect::<Vec<String>>()
            })
            .collect();

        if urls.is_empty() {
            continue;
        }

        let path = format!("sitemap-{}.xml", component.name);

        sitemaps.push((path.clone(), urlset(&urls)));

        let _ = writeln!(
            index,
            "<sitemap><loc>{}</loc></sitemap>",
            escape(&format!("{base}/{path}"))
        );
    }

    index.push_str("</sitemapindex>\n");

    if !sitemaps.is_empty() {
        sitemaps.push(("sitemap.xml".to_string(), index));
    }

    sitemaps
}

/// One component's list of URLs.
fn urlset(urls: &[String]) -> String {
    let mut out = String::from(
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<urlset \
         xmlns=\"http://www.sitemaps.org/schemas/sitemap/0.9\">\n",
    );

    for url in urls {
        let _ = writeln!(out, "<url><loc>{}</loc></url>", escape(url));
    }

    out.push_str("</urlset>\n");

    out
}

/// Escape a URL for XML.
fn escape(url: &str) -> String {
    url.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('\'', "&apos;")
        .replace('"', "&quot;")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_site_with_no_address_has_no_sitemap() {
        let catalog = Catalog::new(antors_model::url::ExtensionStyle::default());

        assert_eq!(sitemaps(&Playbook::default(), &catalog), Vec::new());
    }

    #[test]
    fn ampersands_are_escaped() {
        assert_eq!(escape("https://x/a?b&c"), "https://x/a?b&amp;c");
    }
}
