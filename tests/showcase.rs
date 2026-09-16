//! The showcase site, built and checked.
//!
//! `resources/showcase` is a small but complete Antora site: three components,
//! two versions of one of them, three modules, and a page for every part of the
//! resource model. It is also built by Antora itself — see
//! `resources/showcase/compare.sh` — so what these tests assert is not this
//! implementation's own opinion of what is correct. Each expectation below was
//! read out of Antora's output first.

use std::{
    collections::BTreeSet,
    path::{
        Path,
        PathBuf,
    },
};

use antors_model::Playbook;
use antors_site::{
    Build,
    Options,
    Report,
};

/// Build the showcase into a directory of this test run's own.
fn build(name: &str) -> (PathBuf, Report) {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let playbook_path = root.join("resources/showcase/antora-playbook.yml");

    let mut playbook = Playbook::load(&playbook_path).expect("the showcase playbook loads");

    let out = root.join("target/tests").join(name);
    playbook.output.dir.clone_from(&out);

    let report = Build::new(
        playbook,
        Options {
            render: antors_site::build::RenderOptions::default(),
            clean: true,
        },
    )
    .run()
    .expect("the showcase builds");

    (out, report)
}

/// Every file under `root`, as `/`-separated paths relative to it.
fn tree(root: &Path) -> BTreeSet<String> {
    let mut files = BTreeSet::new();
    let mut pending = vec![root.to_path_buf()];

    while let Some(directory) = pending.pop() {
        let Ok(entries) = std::fs::read_dir(&directory) else {
            continue;
        };

        for entry in entries.flatten() {
            let path = entry.path();

            if path.is_dir() {
                pending.push(path);
            } else if let Ok(relative) = path.strip_prefix(root) {
                files.insert(relative.to_string_lossy().replace('\\', "/"));
            }
        }
    }

    files
}

/// Read one built page.
fn page(out: &Path, path: &str) -> String {
    std::fs::read_to_string(out.join(path)).unwrap_or_else(|_| panic!("`{path}` was written"))
}

#[test]
fn the_site_has_the_files_antora_writes() {
    let (out, _) = build("tree");
    let files = tree(&out);

    for expected in [
        // A versioned component: one directory per version, the module name
        // dropped for ROOT and kept for the others.
        "showcase/2.0/index.html",
        "showcase/2.0/blocks.html",
        "showcase/2.0/guide/getting-started.html",
        "showcase/2.0/api/errors.html",
        "showcase/1.0/legacy.html",
        // An unversioned component has no version segment at all.
        "sidecar/index.html",
        // Images and attachments, under the module that owns them.
        "showcase/2.0/_images/logo.svg",
        "showcase/2.0/guide/_images/screenshot.svg",
        "showcase/2.0/_attachments/report.pdf",
        // `page-aliases` becomes a redirect at each ID it named.
        "showcase/2.0/home.html",
        "showcase/2.0/start.html",
        // The site's own files.
        "index.html",
        "404.html",
        "robots.txt",
        "sitemap.xml",
        "sitemap-showcase.xml",
        "_/css/site.css",
        "_/js/site.js",
    ] {
        assert!(files.contains(expected), "`{expected}` is missing");
    }
}

#[test]
fn nothing_that_is_not_published_is_published() {
    let (out, _) = build("unpublished");
    let files = tree(&out);

    for family in ["partials", "examples", "_partials"] {
        assert!(
            !files.iter().any(|file| file.contains(family)),
            "a {family} file reached the site"
        );
    }

    // A navigation file is read during the build and is not a page.
    assert!(!files.iter().any(|file| file.ends_with("nav.html")));
}

#[test]
fn every_shape_of_resource_id_resolves() {
    let (out, _) = build("xrefs");
    let html = page(&out, "showcase/2.0/resource-ids.html");

    // Each of these was read out of Antora's own build of the same page.
    for expected in [
        // Same module.
        r#"href="blocks.html""#,
        // Same module, into a section.
        r#"href="blocks.html#sidebar""#,
        // Another module.
        r#"href="guide/getting-started.html""#,
        r#"href="api/errors.html""#,
        // Another component, which is unversioned.
        r#"href="../../sidecar/index.html""#,
        // Another version of this component.
        r#"href="../1.0/legacy.html""#,
        // An attachment.
        r#"href="_attachments/report.pdf""#,
    ] {
        assert!(html.contains(expected), "no link to `{expected}`");
    }
}

#[test]
fn a_reference_with_no_text_shows_the_target_page_title() {
    let (out, _) = build("xref-text");
    let html = page(&out, "showcase/2.0/resource-ids.html");

    // `xref:guide:getting-started.adoc[]` — the text comes from the *other*
    // page's title, which is why the build reads every page twice.
    assert!(html.contains(r#"href="guide/getting-started.html">Getting started</a>"#));
    assert!(html.contains(r#"href="../1.0/legacy.html">Legacy notes</a>"#));
}

#[test]
fn images_resolve_across_modules() {
    let (out, _) = build("images");
    let html = page(&out, "showcase/2.0/media.html");

    assert!(html.contains(r#"src="_images/logo.svg""#));
    assert!(html.contains(r#"src="guide/_images/screenshot.svg""#));
}

#[test]
fn includes_are_read_through_the_catalog() {
    let (out, _) = build("includes");
    let html = page(&out, "showcase/2.0/includes.html");

    // `include::partial$shared-warning.adoc[]`
    assert!(html.contains("This is a shared partial"));

    // A partial that reads the component's own attributes.
    assert!(html.contains("2.0.3"));

    // `include::partial$nested-section.adoc[leveloffset=+1]` — a level-0
    // heading in the partial becomes a level-1 section of the page.
    assert!(html.contains(r#"<h2 id="an-included-section""#));

    // A partial included from another module by its full resource ID.
    let guide = page(&out, "showcase/2.0/guide/getting-started.html");
    assert!(guide.contains("This is a shared partial"));
}

#[test]
fn tagged_regions_of_an_example_are_included() {
    let (out, _) = build("tags");
    let html = page(&out, "showcase/2.0/code.html");

    // The highlighter wraps each token in a span of its own, so the assertion
    // is on identifiers rather than on whole expressions.
    assert!(html.contains("TcpListener"), "the whole file");
    assert!(html.contains("Router"), "the tagged region");

    // `tags=site;content` takes two named regions and nothing between them.
    let includes = page(&out, "showcase/2.0/includes.html");
    assert!(includes.contains("Antors Showcase"));
}

#[test]
fn an_alias_redirects_to_the_page_that_replaced_it() {
    let (out, _) = build("aliases");
    let html = page(&out, "showcase/2.0/home.html");

    assert!(html.contains(r#"location="index.html""#));
    assert!(html.contains(r#"content="0; url=index.html""#));
}

#[test]
fn the_site_start_page_redirects_from_the_root() {
    let (out, _) = build("start-page");
    let html = page(&out, "index.html");

    assert!(html.contains(r#"location="showcase/2.0/index.html""#));
}

#[test]
fn the_navigation_is_drawn_from_every_nav_file() {
    let (out, _) = build("nav");
    let html = page(&out, "showcase/2.0/index.html");

    // The ROOT module's list, a group heading in it, and the other two
    // modules' titled lists.
    assert!(html.contains(r#"<a class="nav-link" href="blocks.html">Blocks</a>"#));
    assert!(html.contains(r#"<span class="nav-text">AsciiDoc</span>"#));
    assert!(html.contains(r#"<span class="nav-text">Guide</span>"#));
    assert!(html.contains(r#"<span class="nav-text">API</span>"#));

    // An external entry keeps its URL.
    assert!(html.contains(r#"href="https://docs.antora.org""#));

    // The page being read is marked, and so is the branch it is on.
    let blocks = page(&out, "showcase/2.0/blocks.html");
    assert!(blocks.contains(r#"class="nav-item is-current-page""#));
    assert!(blocks.contains("is-open"));
}

#[test]
fn the_version_selector_offers_every_version() {
    let (out, _) = build("versions");

    // 2.0 has this page and 1.0 has not, so 1.0's entry is marked and points
    // at its start page instead.
    let html = page(&out, "showcase/2.0/blocks.html");
    assert!(html.contains(r#"class="version is-current""#));
    assert!(html.contains("is-missing"));
    assert!(html.contains("2.0 (current)"));

    // An unversioned component has no selector at all.
    let sidecar = page(&out, "sidecar/index.html");
    assert!(!sidecar.contains("page-versions"));
}

#[test]
fn a_page_gets_the_attributes_that_say_where_it_is() {
    let (out, _) = build("attributes");
    let html = page(&out, "showcase/2.0/page-attributes.html");

    for expected in [
        "showcase",             // page-component-name
        "Showcase",             // page-component-title
        "2.0 (current)",        // page-component-display-version
        "ROOT",                 // page-module
        "page-attributes.adoc", // page-relative-src-path
    ] {
        assert!(html.contains(expected), "`{expected}` is missing");
    }
}

#[test]
fn pagination_follows_the_navigation_order() {
    let (out, _) = build("pagination");
    let html = page(&out, "showcase/2.0/index.html");

    // `index.adoc` is the first entry, so it has a next and no previous.
    assert!(html.contains(r#"<span class="next"><a href="text.html">"#));
    assert!(!html.contains(r#"<span class="prev">"#));
}

#[test]
fn the_breadcrumbs_follow_the_navigation_rather_than_the_directories() {
    let (out, _) = build("breadcrumbs");
    let html = page(&out, "showcase/2.0/guide/configuration.html");

    let crumbs = html
        .split("<nav class=\"breadcrumbs\"")
        .nth(1)
        .expect("the page has breadcrumbs");

    assert!(crumbs.contains("Showcase"));
    assert!(crumbs.contains("Guide"));
    assert!(crumbs.contains("Configuration"));
}

#[test]
fn a_clean_showcase_reports_nothing() {
    let (_, report) = build("report");

    assert!(
        report.problems.is_empty(),
        "the showcase should build without complaint, but: {:#?}",
        report.problems
    );

    assert_eq!(report.pages, 17);
}
