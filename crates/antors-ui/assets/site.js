// antors default UI.
//
// Everything here is an enhancement: the page reads, navigates and prints with
// scripting switched off. The navigation tree is rendered open on the path to
// the current page, the outline is rendered into the HTML, and every link is a
// real link — so this file only adds the parts that need a click.

;(function () {
  'use strict'

  var doc = document

  function on (element, event, handler) {
    if (element) element.addEventListener(event, handler)
  }

  function each (selector, fn, root) {
    Array.prototype.forEach.call((root || doc).querySelectorAll(selector), fn)
  }

  // --- the navbar's menu on a narrow screen --------------------------------

  var burger = doc.querySelector('.navbar-burger')
  var menu = doc.querySelector('.navbar-menu')

  on(burger, 'click', function () {
    var open = menu.classList.toggle('is-active')
    burger.setAttribute('aria-expanded', String(open))
  })

  // --- the navigation sidebar on a narrow screen ---------------------------

  var navContainer = doc.querySelector('.nav-container')

  on(doc.querySelector('.nav-toggle'), 'click', function () {
    if (navContainer) navContainer.classList.toggle('is-open')
  })

  // --- expanding and collapsing navigation branches ------------------------

  each('.nav-item-toggle', function (toggle) {
    on(toggle, 'click', function () {
      toggle.parentElement.classList.toggle('is-open')
    })
  })

  // Clicking the text of a group that is not a link opens it too: a heading
  // that does nothing when clicked is a heading that looks broken.
  each('.nav-text', function (text) {
    var item = text.parentElement
    if (!item.querySelector(':scope > .nav-item-toggle')) return

    text.style.cursor = 'pointer'

    on(text, 'click', function () {
      item.classList.toggle('is-open')
    })
  })

  // --- the version selector -------------------------------------------------

  var versions = doc.querySelector('.page-versions')
  var versionToggle = doc.querySelector('.version-menu-toggle')

  on(versionToggle, 'click', function (event) {
    event.stopPropagation()
    var open = versions.classList.toggle('is-open')
    versionToggle.setAttribute('aria-expanded', String(open))
  })

  on(doc.documentElement, 'click', function () {
    if (!versions) return
    versions.classList.remove('is-open')
    if (versionToggle) versionToggle.setAttribute('aria-expanded', 'false')
  })

  on(doc, 'keydown', function (event) {
    if (event.key !== 'Escape') return
    if (versions) versions.classList.remove('is-open')
    if (navContainer) navContainer.classList.remove('is-open')
    if (menu) menu.classList.remove('is-active')
  })

  // --- the explore panel ----------------------------------------------------

  var panels = {
    menu: doc.querySelector('.nav-panel-menu'),
    explore: doc.querySelector('.nav-panel-explore'),
  }

  function showPanel (name) {
    Object.keys(panels).forEach(function (key) {
      if (panels[key]) panels[key].classList.toggle('is-active', key === name)
    })
  }

  on(doc.querySelector('.navbar-explore'), 'click', function (event) {
    event.preventDefault()
    var showing = panels.explore && panels.explore.classList.contains('is-active')
    showPanel(showing ? 'menu' : 'explore')
    if (navContainer) navContainer.classList.add('is-open')
  })

  on(doc.querySelector('.nav-panel-explore .back'), 'click', function () {
    showPanel('menu')
  })

  // --- which outline entry the reader is looking at -------------------------

  // `#toc` is the back end's container, which the shell places in the outline
  // column; `is-active` is the class its stylesheet marks the current entry with.
  var tocLinks = Array.prototype.slice.call(doc.querySelectorAll('#toc a[href^="#"]'))

  if (tocLinks.length) {
    var headings = []

    tocLinks.forEach(function (link) {
      var target = doc.getElementById(decodeURIComponent(link.hash.slice(1)))
      if (target) headings.push({ element: target, link: link })
    })

    // Every entry whose heading is on screen is marked, not just one.
    //
    // A single mark has to answer "which section is the reader in?", and while
    // a section title and its first subsection title are both in view the
    // honest answer is "both" — so a single mark picks one and jumps up a level
    // and back down again as they pass. A run of marks says the same thing
    // without having to choose, and because the gutter beside every entry is
    // already held open, a run of them reads as one bar down the side.
    var active = []

    function readingLine () {
      var toolbar = doc.querySelector('.toolbar')
      return toolbar ? toolbar.getBoundingClientRect().bottom + 8 : 96
    }

    // The headings between the reading line and the foot of the window.
    function onScreen () {
      var line = readingLine()
      var seen = []

      headings.forEach(function (heading) {
        var top = heading.element.getBoundingClientRect().top

        if (top >= line && top <= window.innerHeight) seen.push(heading)
      })

      return seen
    }

    // The last heading the reader has scrolled past, for when none is on
    // screen: an outline with no mark at all says the reader is nowhere, and
    // between two headings a screen apart that would be most of the page.
    function lastPassed () {
      var line = readingLine()
      var passed = headings[0]

      for (var index = 0; index < headings.length; index++) {
        if (headings[index].element.getBoundingClientRect().top > line) break
        passed = headings[index]
      }

      return passed
    }

    function same (one, other) {
      if (one.length !== other.length) return false

      for (var index = 0; index < one.length; index++) {
        if (one[index] !== other[index]) return false
      }

      return true
    }

    function update () {
      if (!headings.length) return

      var current = onScreen()

      if (!current.length) current = [lastPassed()]
      if (same(current, active)) return

      active = current

      tocLinks.forEach(function (link) {
        link.classList.remove('is-active')
      })

      current.forEach(function (heading) {
        heading.link.classList.add('is-active')
      })

      // A long outline scrolls independently, so the marked entries have to be
      // brought into its own view rather than the page's. The first of the run
      // is what to aim at: it is where the reader is, and the rest follow it.
      var first = current[0].link
      var menu = first.closest('.toc.sidebar')

      if (menu && menu.scrollHeight > menu.clientHeight) {
        var entry = first.getBoundingClientRect()
        var frame = menu.getBoundingClientRect()

        if (entry.top < frame.top || entry.bottom > frame.bottom) {
          menu.scrollTop += entry.top - frame.top - frame.height / 3
        }
      }
    }

    var pending = false

    function schedule () {
      if (pending) return
      pending = true

      window.requestAnimationFrame(function () {
        pending = false
        update()
      })
    }

    on(window, 'scroll', schedule)
    on(window, 'resize', schedule)
    update()
  }

  // --- copying a listing ------------------------------------------------------

  if (navigator.clipboard) {
    each('.doc .listingblock > .content', function (content) {
      var code = content.querySelector('pre > code, pre')
      if (!code) return

      var button = doc.createElement('button')
      button.className = 'copy'
      button.type = 'button'
      button.textContent = 'Copy'
      button.setAttribute('aria-label', 'Copy this listing')

      on(button, 'click', function () {
        navigator.clipboard.writeText(code.innerText).then(
          function () {
            button.textContent = 'Copied'
            setTimeout(function () {
              button.textContent = 'Copy'
            }, 1500)
          },
          function () {
            button.textContent = 'Failed'
          }
        )
      })

      content.appendChild(button)
    })
  }

  // --- search -----------------------------------------------------------------

  // The index is pagefind's: a directory of chunks beside a wasm module, both
  // fetched only once somebody means to search. A reader who never uses the box
  // downloads none of it, which is the whole reason the index is not a script
  // in the page.
  var searchBox = doc.querySelector('.search')

  if (searchBox) {
    var searchInput = searchBox.querySelector('.search-input')
    var searchPanel = searchBox.querySelector('.search-results')

    // Where the index is, and what a result's URL is relative to. The shell
    // writes both relative to this page, and the first is resolved against it
    // here rather than used as it stands: a dynamic import inside a classic
    // script resolves against the *script*, which lives somewhere else.
    var indexPath = new URL(searchBox.getAttribute('data-search-index'), doc.baseURI).href
    var resultRoot = searchBox.getAttribute('data-search-root') || ''

    var RESULTS = 8 // pages shown at once
    var HEADINGS = 3 // headings shown under one page

    var loading = null // the import, and then the module it brought
    var filtersLoaded = null // the filters the index offers, once they are asked for
    var components = [] // the components there are to filter by
    var only = null // the one being filtered to, or null for all of them
    var serial = 0 // which query the panel is answering

    // Loaded once, on the first sign that somebody is going to search.
    function load () {
      if (loading) return loading

      loading = import(indexPath + 'pagefind.js').then(function (module) {
        return Promise.resolve(module.options({ basePath: indexPath })).then(function () {
          return module
        })
      })

      return loading
    }

    // What there is to filter by, asked of the index rather than written into
    // the page: a site with one component has nothing to choose between, and
    // gets no chooser.
    //
    // Asked for once and remembered, and waited on before the first results are
    // drawn, so that the chips are there with them rather than appearing
    // underneath a moment later.
    function loadFilters () {
      if (filtersLoaded) return filtersLoaded

      filtersLoaded = load()
        .then(function (module) {
          return module.filters()
        })
        .then(function (available) {
          var byComponent = available && available.component
          var names = byComponent ? Object.keys(byComponent) : []

          if (names.length > 1) components = names.sort()
        })

      return filtersLoaded
    }

    function hide () {
      serial++
      searchPanel.hidden = true
      searchPanel.textContent = ''
    }

    function say (message) {
      searchPanel.textContent = ''
      searchPanel.appendChild(filters())

      var line = doc.createElement('p')
      line.className = 'search-empty'
      line.textContent = message

      searchPanel.appendChild(line)
      searchPanel.hidden = false
    }

    // The component chips, rebuilt with each panel so that which one is active
    // is never a second copy of the answer.
    function filters () {
      var row = doc.createElement('div')
      row.className = 'search-filters'

      if (!components.length) return row

      ;[null].concat(components).forEach(function (name) {
        var chip = doc.createElement('button')
        chip.type = 'button'
        chip.className = 'search-filter'
        chip.textContent = name === null ? 'Everything' : name

        if (only === name) chip.classList.add('is-active')

        on(chip, 'click', function () {
          only = name
          run(searchInput.value.trim())
        })

        row.appendChild(chip)
      })

      return row
    }

    // One result: where it is, what it says, and the headings under it that
    // match — a page of a manual is long, and the section is the answer.
    function entry (data) {
      var item = doc.createElement('li')
      item.className = 'search-result'

      var link = doc.createElement('a')
      link.className = 'search-title'
      link.href = resultRoot + data.raw_url
      link.textContent = (data.meta && data.meta.title) || data.raw_url
      item.appendChild(link)

      var where = [data.meta && data.meta.component, data.meta && data.meta.version]
        .filter(Boolean)
        .join(' · ')

      if (where) {
        var badge = doc.createElement('span')
        badge.className = 'search-where'
        badge.textContent = where
        item.appendChild(badge)
      }

      var excerpt = doc.createElement('p')
      excerpt.className = 'search-excerpt'
      // The only markup in an excerpt is pagefind's own `<mark>`: the text it
      // is built from was escaped before the match was marked in it.
      excerpt.innerHTML = data.excerpt
      item.appendChild(excerpt)

      // A sub-result without an anchor *is* the page, which is already the link
      // above it.
      var headings = (data.sub_results || [])
        .filter(function (sub) {
          return sub.url.indexOf('#') !== -1
        })
        .slice(0, HEADINGS)

      if (headings.length) {
        var list = doc.createElement('ul')
        list.className = 'search-headings'

        headings.forEach(function (sub) {
          var row = doc.createElement('li')
          var anchor = doc.createElement('a')

          // The hash is pagefind's; the path is ours. Its own `url` is rewritten
          // against a base this page has no way to state, so only the part it
          // worked out — which heading matched — is taken from it.
          anchor.href = resultRoot + data.raw_url + sub.url.slice(sub.url.indexOf('#'))
          anchor.textContent = sub.title

          row.appendChild(anchor)
          list.appendChild(row)
        })

        item.appendChild(list)
      }

      return item
    }

    function render (results, term) {
      searchPanel.textContent = ''
      searchPanel.appendChild(filters())

      if (!results.length) {
        var none = doc.createElement('p')
        none.className = 'search-empty'
        none.textContent = 'Nothing matches ' + term + '.'
        searchPanel.appendChild(none)
        searchPanel.hidden = false

        return
      }

      var list = doc.createElement('ul')
      list.className = 'search-list'

      results.forEach(function (data) {
        list.appendChild(entry(data))
      })

      searchPanel.appendChild(list)
      searchPanel.hidden = false
    }

    function run (term) {
      if (!term) return hide()

      var mine = ++serial

      Promise.all([load(), loadFilters()])
        .then(function (loaded) {
          var module = loaded[0]
          var options = only ? { filters: { component: [only] } } : {}

          return module.debouncedSearch(term, options, 140)
        })
        .then(function (search) {
          // Null means somebody typed again while this one was waiting.
          if (!search || mine !== serial) return

          return Promise.all(
            search.results.slice(0, RESULTS).map(function (result) {
              return result.data()
            })
          ).then(function (results) {
            if (mine === serial) render(results, term)
          })
        })
        .catch(function () {
          if (mine === serial) say('The search index could not be loaded.')
        })
    }

    // Fetched on the way to the box rather than after the first keystroke, so
    // that the wasm is usually already here by the time there is a word to
    // search for.
    on(searchInput, 'focus', function () {
      loadFilters().catch(function () {})
    })

    on(searchInput, 'input', function () {
      run(searchInput.value.trim())
    })

    // Down into the results and back out again, so a result can be reached
    // without leaving the keyboard.
    on(searchBox, 'keydown', function (event) {
      if (event.key === 'Escape') return hide()

      if (event.key !== 'ArrowDown' && event.key !== 'ArrowUp') return

      var links = Array.prototype.slice.call(searchPanel.querySelectorAll('a'))
      if (!links.length) return

      event.preventDefault()

      var at = links.indexOf(doc.activeElement)
      var step = event.key === 'ArrowDown' ? 1 : -1
      var next = at === -1 ? (step === 1 ? 0 : links.length - 1) : at + step

      if (next < 0) return searchInput.focus()
      if (next >= links.length) return

      links[next].focus()
    })

    // `/` is where a reader's hand goes on a documentation site, but not while
    // they are already typing somewhere.
    on(doc, 'keydown', function (event) {
      if (event.key !== '/' || event.metaKey || event.ctrlKey || event.altKey) return

      var focused = doc.activeElement
      var tag = focused ? focused.tagName : ''

      if (tag === 'INPUT' || tag === 'TEXTAREA' || tag === 'SELECT') return
      if (focused && focused.isContentEditable) return

      event.preventDefault()
      searchInput.focus()
      searchInput.select()
    })

    on(doc.documentElement, 'click', function (event) {
      if (!searchBox.contains(event.target)) hide()
    })
  }
})()
