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
})()
