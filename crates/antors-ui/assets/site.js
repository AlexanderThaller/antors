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

  if (tocLinks.length && 'IntersectionObserver' in window) {
    var byId = {}

    var targets = tocLinks
      .map(function (link) {
        var target = doc.getElementById(decodeURIComponent(link.hash.slice(1)))
        if (target) byId[target.id] = link
        return target
      })
      .filter(Boolean)

    // A heading counts as "the one being read" once it has passed under the
    // toolbar. Tracking the topmost such heading — rather than whichever
    // crossed the line last — keeps the marker steady when scrolling up.
    var visible = {}

    var observer = new IntersectionObserver(
      function (entries) {
        entries.forEach(function (entry) {
          visible[entry.target.id] = entry.isIntersecting
        })

        var current = null

        for (var index = 0; index < targets.length; index++) {
          var id = targets[index].id

          if (visible[id]) {
            current = id
            break
          }

          if (targets[index].getBoundingClientRect().top < 0) current = id
        }

        tocLinks.forEach(function (link) {
          link.classList.remove('is-active')
        })

        if (current && byId[current]) byId[current].classList.add('is-active')
      },
      { rootMargin: '-96px 0px -70% 0px', threshold: 0 }
    )

    targets.forEach(function (target) {
      observer.observe(target)
    })
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
