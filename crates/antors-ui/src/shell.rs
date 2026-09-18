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
    out.push_str(&outline(page));
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

/// The outline, in the column the content grid holds open for it.
///
/// The markup inside is the back end's — the same outline it would put in a
/// standalone page — so the entries here and the headings they point at cannot
/// describe the document differently. All the shell adds is somewhere to put
/// it: an `<aside>`, which says what the column *is* to a reader moving by
/// landmark, and which the grid places.
fn outline(page: &Page) -> String {
    match &page.toc {
        Some(toc) => format!("<aside class=\"toc sidebar\">\n{toc}</aside>\n"),
        None => String::new(),
    }
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

    out.push_str(&search(page));

    // The explore panel lives in the sidebar on a wide screen, where the
    // sidebar is always visible; this is the way to it when the sidebar is not.
    out.push_str(
        "<a class=\"navbar-item navbar-explore\" href=\"#\" aria-label=\"Browse \
         components\">Components</a>\n",
    );

    out.push_str("</div>\n</div>\n</nav>\n</header>\n");

    out
}

/// The search box, for a build that wrote an index.
///
/// Nothing here searches. The markup is the box, the place the results go, and
/// the two paths the script cannot work out for itself: where the index is, and
/// what a result's URL is relative to. Both are written per page for the same
/// reason every other link on it is — the site does not have to know its own
/// address, so a page still works read out of a subdirectory, a branch preview,
/// or a directory on disk.
///
/// The box is a plain `<input>` outside any `<form>`, so a reader who types and
/// presses Enter before the script has loaded reloads nothing.
fn search(page: &Page) -> String {
    if !page.site.search {
        return String::new();
    }

    let mut out = String::from("<div class=\"navbar-item navbar-search\">\n");

    let _ = writeln!(
        out,
        "<div class=\"search\" role=\"search\" data-search-index=\"{}pagefind/\" \
         data-search-root=\"{}\">",
        attr(&ui_path(&page.root_path)),
        attr(&site_root(&page.root_path)),
    );

    out.push_str(
        "<input class=\"search-input\" type=\"search\" placeholder=\"Search\" aria-label=\"Search \
         this site\" autocomplete=\"off\" spellcheck=\"false\">\n",
    );

    // Empty, and hidden, until there is something to put in it.
    out.push_str("<div class=\"search-results\" hidden></div>\n");

    out.push_str("</div>\n</div>\n");

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
    format!("{}_/", site_root(root_path))
}

/// The path from a page at `root_path` to the site root, ready to have
/// something joined onto it.
///
/// `root_path` is written without a trailing slash, and is empty for a page at
/// the root itself. Both are joined onto by the search script, one result URL
/// at a time, so the slash is settled here rather than in JavaScript.
fn site_root(root_path: &str) -> String {
    if root_path.is_empty() {
        return String::new();
    }

    format!("{root_path}/")
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
    fn a_result_url_is_joined_onto_the_path_to_the_root() {
        // The script does no more than put these together, so the slash has to
        // already be here — and not twice over on a page at the root.
        assert_eq!(
            format!("{}{}", site_root("../.."), "showcase/2.0/index.html"),
            "../../showcase/2.0/index.html",
        );

        assert_eq!(
            format!("{}{}", site_root(""), "showcase/2.0/index.html"),
            "showcase/2.0/index.html",
        );
    }

    #[test]
    fn a_site_with_no_index_gets_no_search_box() {
        let page = Page::default();

        assert!(!page.site.search, "a page is drawn without one by default");
        assert_eq!(search(&page), "");
    }

    #[test]
    fn a_search_box_says_where_the_index_is() {
        let page = Page {
            root_path: "../..".to_string(),
            site: crate::model::Site {
                search: true,
                ..crate::model::Site::default()
            },
            ..Page::default()
        };

        let html = search(&page);

        assert!(
            html.contains(r#"data-search-index="../../_/pagefind/""#),
            "{html}"
        );
        assert!(html.contains(r#"data-search-root="../../""#), "{html}");

        // Outside a form: Enter before the script has loaded must do nothing.
        assert!(!html.contains("<form"));
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
