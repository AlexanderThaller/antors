//! The page: `<head>`, the navbar, the body, the footer.

use std::fmt::Write as _;

use crate::{
    article,
    escape::{
        attr,
        detag,
        text,
    },
    model::Page,
    nav,
    toc,
};

/// Render one page.
pub fn render(page: &Page) -> String {
    let mut out = String::with_capacity(page.content.len() + 8 * 1024);

    out.push_str("<!DOCTYPE html>\n<html lang=\"en\">\n<head>\n");
    out.push_str(&head(page));
    out.push_str("</head>\n");

    let body_class = match &page.role {
        Some(role) => format!("article {role}"),
        None => "article".to_string(),
    };

    let _ = writeln!(out, "<body class=\"{}\">", attr(&body_class));

    out.push_str(&navbar(page));
    out.push_str("<div class=\"body\">\n");
    out.push_str(&nav::sidebar(page));

    out.push_str("<main class=\"article\">\n");
    out.push_str(&article::toolbar(page));
    out.push_str("<div class=\"content\">\n");
    out.push_str(&toc::sidebar(page));
    out.push_str(&article::article(page));
    out.push_str("</div>\n</main>\n");

    out.push_str("</div>\n");
    out.push_str(&footer(page));

    let _ = writeln!(
        out,
        "<script src=\"{}js/site.js\" defer></script>",
        attr(&ui_path(&page.root_path)),
    );

    out.push_str("</body>\n</html>\n");

    out
}

/// The `<head>` of a page.
fn head(page: &Page) -> String {
    let mut out = String::from(
        "<meta charset=\"utf-8\">\n<meta name=\"viewport\" \
         content=\"width=device-width,initial-scale=1\">\n",
    );

    let title = page
        .title
        .as_deref()
        .map_or_else(|| page.site.title.clone(), detag);

    // The site's name after the page's, the way every documentation site does
    // it, so a wall of browser tabs is told apart by its first word.
    let full_title = if page.site.title.is_empty() || title == page.site.title {
        title
    } else {
        format!("{title} :: {}", page.site.title)
    };

    let _ = writeln!(out, "<title>{}</title>", text(&full_title));

    if let Some(url) = &page.canonical_url {
        let _ = writeln!(out, "<link rel=\"canonical\" href=\"{}\">", attr(url));
    }

    if let Some(previous) = &page.previous {
        let _ = writeln!(out, "<link rel=\"prev\" href=\"{}\">", attr(&previous.href));
    }

    if let Some(next) = &page.next {
        let _ = writeln!(out, "<link rel=\"next\" href=\"{}\">", attr(&next.href));
    }

    if let Some(description) = &page.description {
        let _ = writeln!(
            out,
            "<meta name=\"description\" content=\"{}\">",
            attr(&detag(description)),
        );
    }

    if let Some(keywords) = &page.keywords {
        let _ = writeln!(
            out,
            "<meta name=\"keywords\" content=\"{}\">",
            attr(keywords),
        );
    }

    let _ = write!(
        out,
        "<meta name=\"generator\" content=\"antors {}\">\n<link rel=\"stylesheet\" \
         href=\"{}css/site.css\">\n",
        env!("CARGO_PKG_VERSION"),
        attr(&ui_path(&page.root_path)),
    );

    out
}

/// The bar across the top of every page.
fn navbar(page: &Page) -> String {
    let mut out = String::from(
        "<header class=\"header\">\n<nav class=\"navbar\">\n<div class=\"navbar-brand\">\n",
    );

    let home = page.site.home_url.as_deref().unwrap_or("./");

    let _ = writeln!(
        out,
        "<a class=\"navbar-item\" href=\"{}\">{}</a>",
        attr(home),
        text(&page.site.title),
    );

    out.push_str(
        "<button class=\"navbar-burger\" aria-controls=\"topbar-nav\" aria-expanded=\"false\" \
         aria-label=\"Toggle main menu\"><span></span><span></span><span></span></button>\n",
    );

    out.push_str(
        "</div>\n<div id=\"topbar-nav\" class=\"navbar-menu\">\n<div class=\"navbar-end\">\n",
    );

    // The explore panel lives in the sidebar on a wide screen, where the
    // sidebar is always visible; this is the way to it when the sidebar is not.
    out.push_str(
        "<a class=\"navbar-item navbar-explore\" href=\"#\" aria-label=\"Browse \
         components\">Components</a>\n",
    );

    out.push_str("</div>\n</div>\n</nav>\n</header>\n");

    out
}

/// The footer.
fn footer(page: &Page) -> String {
    let mut out = String::from("<footer class=\"footer\">\n");

    let _ = writeln!(
        out,
        "<p>{} — built with <a href=\"https://github.com/AlexanderThaller/antors\">antors</a>.</p>",
        text(&page.site.title),
    );

    out.push_str("</footer>\n");

    out
}

/// Where the UI's own files sit, relative to a page at `root_path`.
fn ui_path(root_path: &str) -> String {
    if root_path.is_empty() {
        return "_/".to_string();
    }

    format!("{root_path}/_/")
}

/// The page served when nothing else matches.
///
/// It is written at the site root and links only to absolute paths, because it
/// is shown for a URL that does not exist and so cannot say how deep it is.
pub fn not_found(site_title: &str, home_url: &str, base: &str) -> String {
    let mut out = String::from("<!DOCTYPE html>\n<html lang=\"en\">\n<head>\n");

    let _ = write!(
        out,
        "<meta charset=\"utf-8\">\n<meta name=\"viewport\" \
         content=\"width=device-width,initial-scale=1\">\n<title>Page Not Found :: \
         {}</title>\n<link rel=\"stylesheet\" href=\"{}_/css/site.css\">\n",
        text(site_title),
        attr(base),
    );

    out.push_str("</head>\n<body class=\"status-404\">\n");

    let _ = write!(
        out,
        "<header class=\"header\">\n<nav class=\"navbar\">\n<div class=\"navbar-brand\">\n<a \
         class=\"navbar-item\" href=\"{}\">{}</a>\n</div>\n</nav>\n</header>\n",
        attr(home_url),
        text(site_title),
    );

    let _ = write!(
        out,
        "<div class=\"body\">\n<main class=\"article\">\n<div class=\"content\">\n<article \
         class=\"doc\">\n<h1 class=\"page\">Page Not Found</h1>\n<div class=\"paragraph\"><p>The \
         page you are looking for is not here. Try <a href=\"{}\">the start \
         page</a>.</p></div>\n</article>\n</div>\n</main>\n</div>\n</body>\n</html>\n",
        attr(home_url),
    );

    out
}

/// A page that exists only to send the reader somewhere else.
///
/// Both a script and a `refresh` are emitted: the script is instant, and the
/// meta refresh is what a reader with scripting switched off gets. The prose is
/// for the reader who gets neither, and names the destination so they can
/// follow it themselves.
pub fn redirect(href: &str, canonical: Option<&str>) -> String {
    let mut out = String::from("<!DOCTYPE html>\n<meta charset=\"utf-8\">\n");

    if let Some(canonical) = canonical {
        let _ = writeln!(out, "<link rel=\"canonical\" href=\"{}\">", attr(canonical));
    }

    let _ = write!(
        out,
        "<script>location=\"{}\"</script>\n<meta http-equiv=\"refresh\" content=\"0; \
         url={}\">\n<meta name=\"robots\" content=\"noindex\">\n<title>Redirect \
         Notice</title>\n<h1>Redirect Notice</h1>\n<p>The page you requested has been relocated \
         to <a href=\"{}\">{}</a>.</p>\n",
        attr(href),
        attr(href),
        attr(href),
        text(canonical.unwrap_or(href)),
    );

    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_ui_path_is_relative_to_the_page() {
        assert_eq!(ui_path(""), "_/");
        assert_eq!(ui_path("../.."), "../../_/");
    }

    #[test]
    fn a_page_title_is_followed_by_the_site_title() {
        let page = Page {
            title: Some("Blocks".to_string()),
            site: crate::model::Site {
                title: "A Site".to_string(),
                ..crate::model::Site::default()
            },
            ..Page::default()
        };

        assert!(head(&page).contains("<title>Blocks :: A Site</title>"));
    }

    #[test]
    fn a_page_with_no_title_is_named_after_the_site() {
        let page = Page {
            site: crate::model::Site {
                title: "A Site".to_string(),
                ..crate::model::Site::default()
            },
            ..Page::default()
        };

        assert!(head(&page).contains("<title>A Site</title>"));
    }

    #[test]
    fn a_title_reaches_the_tab_without_its_markup() {
        let page = Page {
            title: Some("The <code>nav</code> file".to_string()),
            ..Page::default()
        };

        assert!(head(&page).contains("<title>The nav file</title>"));
    }

    #[test]
    fn a_redirect_names_where_it_is_going() {
        let html = redirect("index.html", Some("https://example.org/a/index.html"));

        assert!(html.contains(r#"<script>location="index.html"</script>"#));
        assert!(html.contains(r#"content="0; url=index.html""#));
        assert!(html.contains("https://example.org/a/index.html"));
    }
}
