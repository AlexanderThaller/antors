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

  var tocLinks = Array.prototype.slice.call(doc.querySelectorAll('.toc-menu a[href^="#"]'))

  if (tocLinks.length) {
    var headings = []

    tocLinks.forEach(function (link) {
      var target = doc.getElementById(decodeURIComponent(link.hash.slice(1)))
      if (target) headings.push({ element: target, link: link })
    })

    // The entry to mark is the *last* heading the reader has scrolled past —
    // which only ever moves one way as the page moves one way. Marking whichever
    // heading is currently on screen instead looks right until a section title
    // and its first subsection title arrive together, and then the mark jumps
    // up a level and back down again as they pass.
    var active = null

    function readingLine () {
      var toolbar = doc.querySelector('.toolbar')
      return toolbar ? toolbar.getBoundingClientRect().bottom + 8 : 96
    }

    function update () {
      if (!headings.length) return

      var line = readingLine()
      var current = headings[0]

      for (var index = 0; index < headings.length; index++) {
        if (headings[index].element.getBoundingClientRect().top > line) break
        current = headings[index]
      }

      // The last section of a page may be too short to ever reach the line, so
      // it would be unreachable without this.
      var atBottom =
        window.innerHeight + window.scrollY >= doc.documentElement.scrollHeight - 2

      if (atBottom) current = headings[headings.length - 1]

      if (current === active) return
      active = current

      tocLinks.forEach(function (link) {
        link.classList.remove('is-active')
      })

      current.link.classList.add('is-active')

      // A long outline scrolls independently, so the marked entry has to be
      // brought into its own view rather than the page's.
      var menu = current.link.closest('.toc.sidebar')

      if (menu && menu.scrollHeight > menu.clientHeight) {
        var entry = current.link.getBoundingClientRect()
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
