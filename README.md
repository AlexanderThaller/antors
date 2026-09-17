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
| Playbook | the whole schema parses — `site`, `content`, `urls`, `ui`, `asciidoc`, `output`, `runtime`, `git`, `network`, `antora` |
| Refs | `branches:` and `tags:` from a local repository, read out of git's object database; `worktrees:` decides which come off disk |
| Git LFS | a pointer is followed to the object store, and a file that was never fetched is named rather than published as its pointer |
| Components | multiple components, multiple versions, `display_version`, `prerelease`, `start_page` |
| Versions | from `antora.yml`, or from a content source's `version:` — `true`, a literal, or ref-name patterns |
| Modules | `ROOT` and named modules, with their own images and attachments |
| Resource IDs | `version@component:module:family$path` in every combination |
| References | `xref:` across modules, components and versions; auto text from the target's title |
| Includes | `partial$`, `example$`, tags, line ranges, `leveloffset`, nested includes, bare targets that cross families |
| Media | `image$`, `attachment$`, cross-module images, `imagesdir`/`attachmentsdir` |
| Navigation | several `nav.adoc` per component, nested lists, list titles, external entries |
| Page shell | navbar, navigation sidebar, breadcrumbs, version selector, outline, pagination, edit link |
| Output | `page-aliases` redirects, site start page, `404.html`, `robots.txt`, sitemaps |
| Leftovers | a page in the output directory that this build did not write is reported — a site is written *over* the last one, and a page that moved leaves a copy at its old URL |
| Diagrams | a `[mermaid]` block is drawn while the site is built, so no library is loaded in the browser and it prints |
| Metadata | a document's author, revision, status and tags are shown under its title — see [Beyond Antora](#beyond-antora) |
| Tags | `:page-tags:` gathers into a generated `tags.adoc` per component version |
| Serving | `antors serve` rebuilds and reloads the open page as sources change |

### What does not, yet

Each of these is *reported* rather than skipped quietly: a build says which of
the things the playbook configured it did not do, so a site moved here does not
have to be diffed against Antora's to find out.

- **Remote repositories.** A `url:` must name a directory on this machine.
  Branches and tags within it are read; nothing is cloned or fetched. The seam
  for it is [`antors_content::git`](crates/antors-content/src/git.rs), which
  already reads refs the same way a fetched clone would be read.
- **Antora UI bundles.** The page shell is built in, in
  [`antors-ui`](crates/antors-ui). It follows the default UI's class names, so a
  stylesheet written for Antora mostly applies, but a bundle's Handlebars
  templates are not run.
- **`ui.supplemental_files`.** Parsed, not yet applied.
- **The search index.** No `lunr` index is written.
- **Extensions.** `antora.extensions` and `asciidoc.extensions` are parsed and
  named in the report; they are Node modules and are not run. A site that
  generates pages from an extension will be missing those pages.

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
| `--no-tags-page` | Do not generate the page that gathers every `:page-tags:` entry. |

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

## Beyond Antora

Two things here are deliberately not what Antora does.

### What a document says about itself

A header carries two sorts of thing, and only one is for the reader.
`:sectnums:` is an instruction to the renderer; an author, a revision, a status
and a set of tags are *about the document*, and someone opening a design note
wants to know when it was written and whether it still stands. So a page like

```asciidoc
= Kubernetes Operator
Alexander Thaller <alexander@thaller.ws>
v1.0, 2026-09-10
:status: living document
:page-tags: kubernetes, operator
```

is shown with those facts under its title, labelled. Which attributes count as
facts is [`adocers-render-core`]'s answer rather than one of this project's, so
a page here and a PDF of the same document say the same things under the same
labels.

Antora's own `page-` attributes are instructions rather than facts —
`page-role`, `page-aliases`, `page-toclevels`, `page-edit-url`, `page-partial`,
`page-layout`, and the `page-component-*` family the build sets itself — and are
left out. Everything else in the namespace is the author's own and is shown.

[`adocers-render-core`]: https://crates.io/crates/adocers-render-core

### The stylesheet

The article's stylesheet is not written here. It is [`adocers-html`]'s, used
rather than imitated:

```
adocers_html::stylesheet_variables()   /* at the top level */
shell.css                              /* this project's frame */
article.doc { adocers_html::document_stylesheet() }
```

The back end's markup is styled by those rules, and a mermaid diagram it draws
carries theme overrides written against the custom properties they declare. A
stylesheet that redefined the article in its own terms — as this one once did —
leaves `fill: var(--code-bg)` resolving to nothing and paints every diagram
node solid black. So the properties are declared at the top level where a
diagram can reach them, and the document rules go in *nested and untouched*,
which is what makes it reuse rather than a copy with a different bug in it.

Only the frame is this project's: the navbar, the navigation tree, the toolbar,
the outline beside the text. That, and the handful of things the back end does
not style — a keyboard key, a button, a link to an attachment.

[`adocers-html`]: https://crates.io/crates/adocers-html

### The tags page

A component version whose pages carry `:page-tags:` gets a `tags.adoc`,
generated *before* anything renders — so it is a page like any other. It can be
cross-referenced, listed in a navigation file, and read with an outline beside
it, and its entries are real cross-references, so a tagged page that is deleted
becomes a reported broken reference rather than a dead link. Each tag on a page
links to its section.

A `tags.adoc` that already exists is treated as the introduction and the list is
added below it, so a placeholder page — the shape an Antora extension needs —
keeps working with nothing removed.

Spellings of one tag are one tag: `Session Store`, `session-store` and
`session store` share a section, titled with whichever spelling is most used. An
index split three ways, each showing a third of the pages and none of them
saying so, would be worse than no index.

`--no-tags-page` leaves it out.

## Performance

A build is almost entirely allocation: every page is parsed into a tree of owned
strings and rendered into another. So the binary brings its own allocator, and
`mimalloc`'s `override` feature is on — which matters more than it sounds,
because most of those allocations are not Rust's. The tree-sitter grammars are C
and call `malloc` themselves, and routing only the Rust half leaves the larger
half where it was.

Measured against a 232-page site, best of three. The musl binary here was
dynamically linked, so these four compare allocators and not linkage:

| | |
| --- | --- |
| glibc, system allocator | 821 ms |
| glibc, mimalloc | 739 ms |
| musl, mimalloc without `override` | 1697 ms |
| musl, mimalloc with `override` | 850 ms |

The musl row is the one worth keeping: it is the usual "musl is slow" report,
and it is not the libc — it is the C allocations nothing had routed.

The container is a *static* musl build, which is a different binary again, so it
was checked on its own: a 240-page site, median of nine runs interleaved between
the two so that any drift lands on both.

| | |
| --- | --- |
| glibc, mimalloc with `override` | 184 ms |
| musl static, mimalloc with `override` | 207 ms |

So the image costs about 12% of a build and saves every shared object it would
otherwise have to carry. That the gap is 12% and not the 130% of the unrouted
row is the standing check that `override` is still winning under a static link;
if it ever regresses towards that, this is what broke.

## How it fits together

The AsciiDoc back end is [adocers](https://github.com/AlexanderThaller/adocers),
taken from the registry like any other dependency. To work on both at once,
point cargo at a sibling checkout with a `[patch.crates-io]` entry rather than
editing the workspace manifest, so that what is committed stays buildable by
anyone.

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
