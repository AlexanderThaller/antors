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
            tags_page: true,
            pdf: false,
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
fn a_bare_include_target_may_climb_into_another_family() {
    let (out, _) = build("bare-include");
    let html = page(&out, "showcase/2.0/includes.html");

    // `include::../examples/playbook.yml[tag=site]` from a page: the `..`
    // leaves `pages/`, and `examples/` is what decides the family.
    assert!(
        html.contains("Antors Showcase"),
        "the example was not included"
    );
}

#[test]
fn tagged_regions_of_an_example_are_included() {
    let (out, _) = build("example-tags");
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

    // `modules/ROOT/nav.adoc` has no list title, so its entries are top-level
    // siblings of the titled files' groups rather than a level deeper — and the
    // container they arrive in gets no expand arrow of its own.
    let menu = html
        .split("<nav class=\"nav-menu\">")
        .nth(1)
        .and_then(|menu| menu.split("</nav>").next())
        .expect("the page has a navigation menu");

    let top_level: Vec<&str> = menu
        .split("<ul class=\"nav-list\">")
        .nth(1)
        .expect("the menu has a list")
        .split("<li class=\"nav-item")
        .skip(1)
        .collect();

    assert!(
        top_level
            .first()
            .is_some_and(|first| first.contains("Introduction")),
        "the first entry should be Introduction, not an unnamed container"
    );

    // An arrow is always followed by the entry it expands — a link or a piece
    // of text. One followed straight by the nested list has nothing beside it,
    // and appears to belong to the entry above.
    for after in menu.split("</button>").skip(1) {
        assert!(
            after.trim_start().starts_with("<a ") || after.trim_start().starts_with("<span "),
            "an expand arrow with nothing beside it, before: {}",
            &after[..after.len().min(60)]
        );
    }
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
fn a_document_states_its_facts_under_its_title() {
    let (out, _) = build("details");
    let html = page(&out, "showcase/2.0/metadata.html");

    let details = html
        .split("<div class=\"details\">")
        .nth(1)
        .and_then(|block| block.split("</div>\n</div>").next())
        .expect("the page has a details block");

    // The author line, the revision line, and the header attributes that are
    // facts rather than instructions.
    assert!(details.contains(r#"<span class="label">Author:</span>"#));
    assert!(details.contains(r#"href="mailto:alexander@thaller.ws">Alexander Thaller</a>"#));
    assert!(details.contains(r#"<span class="label">Version:</span>"#));
    assert!(details.contains("1.0"));
    assert!(details.contains(r#"<span class="label">Date:</span>"#));
    assert!(details.contains("2026-09-10"));
    assert!(details.contains(r#"<span class="label">Status:</span>"#));
    assert!(details.contains("living document"));

    // Each tag is its own mark, and leads to the overview of everything
    // carrying it.
    assert!(details.contains(r#"href="tags.html#tag-asciidoc"><span class="tag">asciidoc</span>"#));
    assert!(details.contains(r#"<span class="tag">rendering</span>"#));
    assert!(details.contains(r#"<span class="tag">showcase</span>"#));
}

#[test]
fn antoras_own_page_attributes_are_not_shown_as_facts() {
    let (out, _) = build("no-directives");

    // This page sets `page-role`, `page-toclevels`, `page-edit-url` and
    // `page-tags`. Only the last is something a reader wants to read.
    let html = page(&out, "showcase/2.0/page-attributes.html");

    let details = html
        .split("<div class=\"details\">")
        .nth(1)
        .and_then(|block| block.split("</div>\n</div>").next())
        .expect("the page has a details block");

    assert!(details.contains(r#"<span class="label">Tags:</span>"#));

    for absent in ["Role:", "Toclevels:", "Edit url:", "Component name:"] {
        assert!(!details.contains(absent), "`{absent}` should not be shown");
    }
}

#[test]
fn a_mermaid_block_is_drawn_while_the_site_is_built() {
    let (out, _) = build("mermaid");
    let html = page(&out, "showcase/2.0/diagrams.html");

    // Drawn, not deferred: the diagram is in the HTML that is served, so it
    // needs no library in the browser and is there for a reader who prints the
    // page.
    assert!(html.contains(r#"<div class="imageblock diagram">"#));
    assert!(html.contains("<svg"), "the diagram was not drawn");
    assert!(!html.contains("mermaid.esm"), "no library should be loaded");

    // Its theme is written against the stylesheet's custom properties, so a
    // stylesheet that did not declare them would leave the nodes unpainted.
    let css =
        std::fs::read_to_string(out.join("_/css/site.css")).expect("the stylesheet was written");

    for property in ["--code-bg", "--rule", "--fg", "--accent"] {
        assert!(
            css.contains(&format!("{property}:")),
            "`{property}` is used by a drawn diagram and must be declared"
        );
    }
}

#[test]
fn a_tagged_component_version_gets_a_tags_page() {
    let (out, _) = build("tags");
    let html = page(&out, "showcase/2.0/tags.html");

    // It is a page like any other: it went through the whole pipeline, so it
    // has a title, a shell, an outline and working cross-references.
    assert!(html.contains(r#"<h1 class="page">Tags</h1>"#));
    assert!(html.contains(r#"<aside class="toc sidebar""#));
    assert!(html.contains(r#"<nav class="breadcrumbs""#));

    // A section per tag, anchored so a tag on a page can link straight to it.
    assert!(html.contains(r#"id="tag-asciidoc""#));
    assert!(html.contains(r#"id="tag-showcase""#));

    // Its entries are real cross-references: resolved, and named after the
    // pages they point at.
    assert!(html.contains(r#"href="metadata.html">Document metadata</a>"#));
    assert!(html.contains(r#"href="guide/getting-started.html">Getting started</a>"#));
}

#[test]
fn a_component_version_with_no_tags_gets_no_tags_page() {
    let (out, _) = build("no-tags");

    // Nothing in 1.0 or in `sidecar` is tagged, and a link to an empty
    // overview is worse than no link.
    assert!(!out.join("showcase/1.0/tags.html").exists());
    assert!(!out.join("sidecar/tags.html").exists());
}

#[test]
fn the_tags_page_can_be_switched_off() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let mut playbook = Playbook::load(&root.join("resources/showcase/antora-playbook.yml"))
        .expect("the showcase playbook loads");

    let out = root.join("target/tests/tags-off");
    playbook.output.dir.clone_from(&out);

    Build::new(
        playbook,
        Options {
            render: antors_site::build::RenderOptions::default(),
            clean: true,
            tags_page: false,
            pdf: false,
        },
    )
    .run()
    .expect("the showcase builds");

    assert!(!out.join("showcase/2.0/tags.html").exists());

    // The tags themselves are still shown; they simply lead nowhere.
    let html = std::fs::read_to_string(out.join("showcase/2.0/metadata.html"))
        .expect("the page was written");

    assert!(html.contains(r#"<span class="tag">asciidoc</span>"#));
    assert!(!html.contains("tags.html"));
}

#[test]
fn a_page_left_over_from_an_earlier_build_is_reported() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let out = root.join("target/tests/stale");

    // A page at a URL this build does not use — what a component that has just
    // been given versions leaves behind at every one of its old URLs.
    let leftover = out.join("showcase/moved-away.html");
    std::fs::create_dir_all(leftover.parent().expect("it has a parent"))
        .expect("the directory is created");
    std::fs::write(&leftover, "<html>from an earlier build</html>").expect("the file is written");

    let mut playbook = Playbook::load(&root.join("resources/showcase/antora-playbook.yml"))
        .expect("the showcase playbook loads");

    playbook.output.dir.clone_from(&out);

    let report = Build::new(
        playbook,
        Options {
            render: antors_site::build::RenderOptions::default(),

            // Not cleaning is the whole point: a build that emptied the
            // directory could not leave anything behind to find.
            clean: false,
            tags_page: true,
            pdf: false,
        },
    )
    .run()
    .expect("the showcase builds");

    let complaint = report
        .problems
        .iter()
        .find(|problem| problem.file == "output")
        .unwrap_or_else(|| panic!("the leftover page was not reported: {:#?}", report.problems));

    assert!(complaint.message.contains("moved-away.html"), "{complaint}");
    assert!(complaint.message.contains("--clean"), "{complaint}");

    // It is still there: the build says so rather than deleting what it did
    // not put there.
    assert!(leftover.exists());
}

#[test]
fn a_clean_showcase_reports_nothing_about_its_content() {
    let (_, report) = build("report");

    let content: Vec<&antors_site::Problem> = report
        .problems
        .iter()
        .filter(|problem| problem.file != "playbook")
        .collect();

    assert!(
        content.is_empty(),
        "the showcase should build without complaint, but: {content:#?}"
    );

    assert_eq!(report.pages, 20);
}

#[test]
fn the_playbook_is_told_what_this_build_does_not_do() {
    let (_, report) = build("notices");

    let playbook: Vec<&str> = report
        .problems
        .iter()
        .filter(|problem| problem.file == "playbook")
        .map(|problem| problem.message.as_str())
        .collect();

    // The showcase playbook names a UI bundle, because Antora builds the same
    // playbook and needs one. This build does not use it, and says so rather
    // than leaving the reader to wonder why the page looks different.
    assert_eq!(
        playbook.len(),
        1,
        "expected only the UI bundle notice, got {playbook:#?}"
    );

    assert!(playbook[0].contains("UI bundle"), "{}", playbook[0]);
}

/// The PDFs.
///
/// Off unless the build is asked for them, which is what every test above
/// relies on: Typst lays out every page from scratch, and a suite that paid for
/// that on each build would be a suite nobody runs.
#[cfg(feature = "pdf")]
mod pdf {
    use super::{
        Build,
        Options,
        Path,
        PathBuf,
        Playbook,
        Report,
        page,
        tree,
    };

    /// The one page of the showcase that will not typeset, and why.
    ///
    /// `adocers-typst` 0.3 writes unconstrained bold as `*b*old`. Typst reads
    /// that as an unclosed delimiter: a `*` with a word character after it does
    /// not close strong emphasis, so the whole document is refused.
    /// `text.adoc` writes `Unconstrained: **b**old, __i__talic`, and is
    /// therefore the one page here that has no PDF.
    ///
    /// Delete this and the allowances below when the back end emits
    /// `#strong[b]old` instead.
    const WILL_NOT_TYPESET: &str = "showcase/2.0/text.pdf";

    /// Build the showcase with the PDFs switched on.
    fn build(name: &str) -> (PathBuf, Report) {
        let root = Path::new(env!("CARGO_MANIFEST_DIR"));

        let mut playbook = Playbook::load(&root.join("resources/showcase/antora-playbook.yml"))
            .expect("the showcase playbook loads");

        let out = root.join("target/tests").join(name);
        playbook.output.dir.clone_from(&out);

        let report = Build::new(
            playbook,
            Options {
                render: antors_site::build::RenderOptions::default(),
                clean: true,
                tags_page: true,
                pdf: true,
            },
        )
        .run()
        .expect("the showcase builds");

        (out, report)
    }

    /// Every PDF this build typeset, which is not every PDF it wrote: the
    /// showcase publishes one as an attachment, and that one is copied.
    fn typeset(out: &Path) -> Vec<String> {
        tree(out)
            .into_iter()
            .filter(|path| {
                Path::new(path)
                    .extension()
                    .is_some_and(|extension| extension == "pdf")
                    && !path.contains("_attachments")
            })
            .collect()
    }

    /// Whether a file is a PDF, rather than merely being called one.
    fn is_a_pdf(out: &Path, path: &str) -> bool {
        let bytes = std::fs::read(out.join(path))
            .unwrap_or_else(|error| panic!("`{path}` was written: {error}"));

        bytes.starts_with(b"%PDF") && bytes.len() > 1000
    }

    #[test]
    fn a_build_that_was_not_asked_for_them_writes_none() {
        let (out, _) = super::build("no-pdfs");

        assert!(typeset(&out).is_empty(), "{:#?}", typeset(&out));
        assert!(!page(&out, "showcase/2.0/media.html").contains("page-pdf"));
    }

    #[test]
    fn every_page_is_typeset_beside_its_html() {
        let (out, report) = build("pdfs");

        for path in [
            "showcase/2.0/index.pdf",
            "showcase/2.0/media.pdf",
            "showcase/2.0/guide/getting-started.pdf",
            "showcase/2.0/api/overview.pdf",
            "showcase/1.0/legacy.pdf",
            "sidecar/index.pdf",
        ] {
            assert!(is_a_pdf(&out, path), "`{path}` is not a PDF");
        }

        // One per page, less the one the back end will not take, plus the two
        // component versions that have a manual. Counted rather than listed so
        // that a page added to the showcase has to be accounted for here.
        assert!(!out.join(WILL_NOT_TYPESET).exists());
        assert_eq!(typeset(&out).len(), report.pages - 1 + 2);
    }

    #[test]
    fn a_page_the_back_end_will_not_take_costs_only_itself() {
        let (out, report) = build("pdf-refused");

        let refused: Vec<&str> = report
            .problems
            .iter()
            .filter(|problem| {
                problem
                    .message
                    .starts_with("this page could not be typeset")
            })
            .map(|problem| problem.message.as_str())
            .collect();

        assert_eq!(refused.len(), 1, "{refused:#?}");

        // The page it could not take is named against itself, and the manual
        // says what that cost rather than silently coming up a page short.
        assert!(
            report
                .problems
                .iter()
                .any(|problem| problem.message.contains("missing from this version\'s PDF")),
            "{:#?}",
            report.problems
        );

        // And the manual was still made, out of the pages that did typeset.
        assert!(is_a_pdf(&out, "showcase/2.0/showcase-2.0.pdf"));
    }

    #[test]
    fn every_component_version_of_more_than_one_page_is_typeset_whole() {
        let (out, _) = build("manuals");

        assert!(is_a_pdf(&out, "showcase/2.0/showcase-2.0.pdf"));
        assert!(is_a_pdf(&out, "showcase/1.0/showcase-1.0.pdf"));

        // A manual of one page would be that page under a second name.
        assert!(!out.join("sidecar/sidecar.pdf").exists());
    }

    #[test]
    fn a_manual_holds_every_page_of_its_version() {
        let (out, _) = build("manual-contents");

        // Bigger than any one page of it, and bigger than all of them would be
        // if it had only picked one up.
        let manual = std::fs::metadata(out.join("showcase/1.0/showcase-1.0.pdf"))
            .expect("the manual was written")
            .len();

        let one = std::fs::metadata(out.join("showcase/1.0/legacy.pdf"))
            .expect("the page was written")
            .len();

        assert!(
            manual > one,
            "the manual is no bigger than one of its pages"
        );
    }

    #[test]
    fn a_page_offers_both_of_its_pdfs() {
        let (out, _) = build("pdf-buttons");
        let html = page(&out, "showcase/2.0/guide/getting-started.html");

        assert!(html.contains(r#"<div class="page-pdf">"#), "{html}");
        assert!(
            html.contains(r#"href="getting-started.pdf" download="Getting started.pdf""#),
            "the page\'s own PDF is not offered"
        );

        // Written from where the reader is standing, like every other link in
        // the shell.
        assert!(
            html.contains(r#"href="../showcase-2.0.pdf" download="Showcase 2.0 (current).pdf""#),
            "the manual is not offered from the page it is offered on"
        );
    }

    #[test]
    fn a_page_with_no_pdf_offers_none() {
        let (out, _) = build("pdf-refused-button");
        let html = page(&out, "showcase/2.0/text.html");

        assert!(!html.contains("pdf-page"), "{html}");

        // The manual is still there to offer, and still offered.
        assert!(html.contains("pdf-manual"), "{html}");
    }

    #[test]
    fn a_component_version_of_one_page_offers_only_that_page() {
        let (out, _) = build("pdf-one-page");
        let html = page(&out, "sidecar/index.html");

        assert!(
            html.contains(r#"href="index.pdf" download="Sidecar.pdf""#),
            "{html}"
        );
        assert!(!html.contains("pdf-manual"), "{html}");
    }
}
