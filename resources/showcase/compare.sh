#!/usr/bin/env bash
#
# Build the showcase with Antora and with antors, and diff the two.
#
# The article body is what is compared — the page shell around it is this
# project's own and is meant to differ. Differences that are known and
# deliberate are normalized away first, so anything this prints is either a new
# divergence or one that has been fixed and should come off the list below.
#
# Needs Node for Antora; it is the only thing in this repository that does.

set -euo pipefail

here="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
root="$(cd "$here/../.." && pwd)"

antora_out="$here/build/antora"
antors_out="$here/build/antors"

echo "==> antora"
(cd "$here" && npx --yes antora@3.2 --fetch --clean --to-dir build/antora antora-playbook.yml)

echo "==> antors"
(cd "$root" && cargo run --release --quiet -- \
  --clean -o "$antors_out" "$here/antora-playbook.yml")

# Pull the article out of a page, dropping the shell around it.
article() {
  python3 - "$1" <<'PY'
import re, sys, pathlib
src = pathlib.Path(sys.argv[1]).read_text()
# Whatever else is on the tag: antors also says there what a search index
# should read, which is the shell's business and not the body's.
match = re.search(r'<article class="doc[^"]*"[^>]*>\n(.*)\n</article>', src, re.S)
if not match:
    sys.exit(0)
body = match.group(1)
body = re.sub(r'^<h1 class="page">.*?</h1>\n', '', body, flags=re.S)
body = re.sub(r'<nav class="pagination"[^>]*>.*?</nav>\n?', '', body, flags=re.S)
sys.stdout.write(body)
PY
}

# Normalize the differences that are known and not bugs.
normalize() {
  # The details block is this project's own: a document's author, revision,
  # status and tags shown under its title. Antora renders none of it.
  perl -0pe 's{<div class="details">.*?</div>\n</div>\n}{}s' | sed \
    -e 's|<a href="\([^"]*\)" class="\([^"]*\)">|<a class="\2" href="\1">|g' \
    -e 's|<pre tabindex="0">|<pre>|g' \
    -e 's|<i class="fa icon-[a-z]*" title="[A-Za-z]*"></i>|ADMONITION-ICON|' \
    -e 's|<svg class="icon icon-[a-z]*".*</svg>|ADMONITION-ICON|' \
    -e 's|<i class="fa fa-[a-z-]*"></i>|CHECKBOX|' \
    -e 's|&#10063;|CHECKBOX|; s|&#10003;|CHECKBOX|' \
    -e 's|class="highlightjs highlight"|class="highlight"|' \
    -e 's| hljs"|"|' \
    -e 's|<span class="hl-[a-z]*">||g' \
    -e 's|</span>||g' \
    -e 's|&quot;|"|g' \
    -e "s|&#39;|'|g"
}

status=0

# Pages that exist only on one side, or that are deliberately rendered
# differently. Both are covered by the integration tests instead; here they
# would only ever read as a difference.
#
#   tags.html     antors generates it and Antora does not, so it is missing
#                 from the tree on one side as well as from the comparison.
#   diagrams.html both sides publish it; only its body differs, because a
#                 `[mermaid]` block is drawn as an SVG here and left as the
#                 listing it was written as by an Antora without the extension.
only_ours='/tags\.html$'
skip_body='/(tags|diagrams)\.html$'

echo "==> file tree"
if diff \
  <(cd "$antora_out" && find . -type f | grep -v '^\./_/' | sort) \
  <(cd "$antors_out" && find . -type f | grep -v '^\./_/' | grep -Ev "$only_ours" | sort)
then
  echo "    identical"
else
  status=1
fi

echo "==> article bodies"
for page in $(cd "$antora_out" && find . -name '*.html' -not -name 404.html | sort); do
  if printf '%s' "$page" | grep -Eq "$skip_body"; then continue; fi

  a="$(article "$antora_out/$page" | normalize)"
  b="$(article "$antors_out/$page" | normalize)"

  if [ "$a" != "$b" ]; then
    echo "--- $page"
    diff <(printf '%s\n' "$a") <(printf '%s\n' "$b") || true
    status=1
  fi
done

[ "$status" = 0 ] && echo "    identical"

exit "$status"
