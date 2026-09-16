# antors

Build and serve an [Antora](https://antora.org) documentation site, in Rust.

`antors` reads the same `antora-playbook.yml` and the same `antora.yml` that
Antora reads, resolves the same resource IDs, and writes the same files to the
same paths — so an existing site can be built with it without changing a line
of AsciiDoc.

The AsciiDoc itself is parsed by
[`asciidoc-parser`](https://github.com/asciidoc-rs/asciidoc-parser) and
rendered by [`adocers-html`](https://crates.io/crates/adocers-html). Nothing
here re-implements AsciiDoc; what it adds is Antora's model on top of it.

## What works

| | |
| --- | --- |
| Playbook | `site`, `content.sources`, `urls`, `asciidoc.attributes`, `output`, `runtime` |
| Components | multiple components, multiple versions, `display_version`, `prerelease`, `start_page` |
| Modules | `ROOT` and named modules, with their own images and attachments |
| Resource IDs | `version@component:module:family$path` in every combination |
| References | `xref:` across modules, components and versions; auto text from the target's title |
| Includes | `partial$`, `example$`, tags, line ranges, `leveloffset`, nested includes |
| Media | `image$`, `attachment$`, cross-module images, `imagesdir`/`attachmentsdir` |
| Navigation | several `nav.adoc` per component, nested lists, list titles, external entries |
| Page shell | navbar, navigation sidebar, breadcrumbs, version selector, outline, pagination, edit link |
| Output | `page-aliases` redirects, site start page, `404.html`, `robots.txt`, sitemaps |
| Serving | `antors serve` rebuilds and reloads the open page as sources change |

### What does not, yet

- **Remote content sources.** A `url:` must name a directory on this machine.
  Antora's branch, tag and worktree selection is not implemented; a source is
  read from the worktree it points at. The seam for it is
  [`antors_content::aggregate`](crates/antors-content/src/aggregate.rs), which
  produces a `Catalog` that nothing else's shape depends on.
- **Antora UI bundles.** The page shell is built in, in
  [`antors-ui`](crates/antors-ui). It follows the default UI's class names, so a
  stylesheet written for Antora mostly applies, but a bundle's Handlebars
  templates are not run.
- **The search index.** No `lunr` index is written.
- **Antora extensions.** A playbook's `antora.extensions` is parsed and ignored;
  they are Node modules.

## Install

```
cargo install --path .
```

## Usage

```
antors [PLAYBOOK]                 # same as `antors build`
antors build [OPTIONS] [PLAYBOOK]
antors serve [OPTIONS] [PLAYBOOK]
```

`PLAYBOOK` defaults to `antora-playbook.yml`.

### build

```
antors                            # build antora-playbook.yml
antors -o build/site --clean      # somewhere else, from scratch
antors --strict                   # fail the build on a warning
```

| Option | Effect |
| --- | --- |
| `-o, --to-dir <DIR>` | Write the site here instead of where the playbook says. |
| `--clean` | Empty the output directory first. |
| `--strict` | Exit non-zero if anything at all was reported. |
| `-q, --quiet` | Say nothing unless something went wrong. |
| `--no-highlight` | Leave source blocks unhighlighted. |
| `--no-mermaid` | Show mermaid diagrams as the listings they were written as. |
| `--no-math` | Show equations as the notation they were written in. |
| `--no-icons` | Mark admonitions with their label instead of an icon. |

### serve

```
antors serve                      # build, serve on 127.0.0.1:4000, rebuild on change
antors serve --bind 0.0.0.0:8080
antors serve --no-watch
```

Every page is served with a small script that waits — without polling — for the
next rebuild and then reloads. The script is added as the page is *served*, so
the site on disk is the same whether it was built to be served or to be
published.

## How it fits together

```
antors            the command line
├── antors-site   the build: what to render, in what order, and where it goes
├── antors-ui     the page shell, and the stylesheet and script that make it work
├── antors-asciidoc  the parser's Antora seams: includes, references, images
├── antors-content   collecting content sources into one catalog
└── antors-model     resource IDs, playbooks, descriptors, URL arithmetic
```

Two things are worth knowing about the shape of a build.

**Every page is read twice.** A reference with no text of its own shows the
*target page's* title, so no page's references can be resolved until every
page's title is known. The first pass learns the titles and the aliases; the
second renders. Holding every parsed document in memory between the two would
cost more than parsing twice.

**Nothing but `antors-content` looks at a directory.** A reference is resolved
against the catalog, an include is read through it, and a page's URL is its
answer — so where content came from is one crate's problem rather than the whole
build's.

## The showcase

`resources/showcase` is a complete Antora site — three components, two versions
of one of them, three modules, and a page for each part of the resource model.
It is the reference the implementation is checked against:

```
cargo test --test showcase          # build it and check what came out
./resources/showcase/compare.sh     # build it with Antora too, and diff the two
```

`compare.sh` needs Node, which is the only thing in this repository that does.
It compares the *article body* of every page — the shell around it is this
project's own and is meant to differ — and normalizes away the handful of
differences that are known and deliberate. Anything it prints is either a new
divergence or one that has been fixed and should come off its list.

## Known divergences from Antora

All of these are in the AsciiDoc back end rather than in the Antora model, and
all are visible in `compare.sh`:

| | |
| --- | --- |
| Admonition icons | An inline SVG rather than a Font Awesome `<i>`. Deliberate. |
| Checklist markers | A character rather than a Font Awesome `<i>`. Deliberate. |
| Callout lists | An `<ol>` rather than a two-column `<table>`. Deliberate. |
| Syntax highlighting | Done while the site is built, with tree-sitter, rather than in the reader's browser with `highlight.js`. Deliberate. |
| `<pre tabindex="0">` | A scrollable listing is focusable. Deliberate. |
| Horizontal description lists | A `[horizontal]` list that follows a nested description list is absorbed into it. A bug. |
| Example captions | An admonition-styled example block advances the example counter. A bug. |
| `image::x[window=_blank]` | The `window` attribute is dropped. A bug. |
| `video::ID[vimeo]` | The provider is ignored and a plain `<video>` is rendered. Missing. |

## License

MIT OR Apache-2.0.
