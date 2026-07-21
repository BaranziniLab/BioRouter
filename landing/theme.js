/* UCSF Biorouter — light/dark theme + footer newsletter.
   Loaded blocking in <head> so the saved theme applies before first paint. */
(function () {
  'use strict';

  // 1. Apply the saved (or system) theme immediately, before the body paints.
  try {
    var saved = localStorage.getItem('br-theme');
    var wantDark = saved
      ? saved === 'dark'
      : (window.matchMedia && window.matchMedia('(prefers-color-scheme: dark)').matches);
    if (wantDark) document.documentElement.classList.add('dark');
  } catch (e) {}

  // 2. Wire the toggle and the newsletter once the DOM is ready.
  function wire() {
    var btn = document.getElementById('theme-toggle');
    if (btn) {
      var sync = function () {
        var dark = document.documentElement.classList.contains('dark');
        btn.setAttribute('aria-pressed', String(dark));
        btn.setAttribute('aria-label', dark ? 'Switch to light theme' : 'Switch to dark theme');
      };
      sync();
      btn.addEventListener('click', function () {
        var dark = document.documentElement.classList.toggle('dark');
        try { localStorage.setItem('br-theme', dark ? 'dark' : 'light'); } catch (e) {}
        sync();
      });
    }

    // Follow the system theme only while the user has not chosen one.
    if (window.matchMedia) {
      var mq = window.matchMedia('(prefers-color-scheme: dark)');
      var onSystem = function (e) {
        var chosen;
        try { chosen = localStorage.getItem('br-theme'); } catch (err) {}
        if (chosen) return;
        document.documentElement.classList.toggle('dark', e.matches);
        var b = document.getElementById('theme-toggle');
        if (b) b.setAttribute('aria-pressed', String(e.matches));
      };
      if (mq.addEventListener) mq.addEventListener('change', onSystem);
    }

    // Newsletter: no backend, so confirm honestly and point to real channels.
    var form = document.getElementById('news-form');
    if (form) {
      form.addEventListener('submit', function (e) {
        e.preventDefault();
        var note = document.getElementById('news-note');
        if (note) note.hidden = false;
        var input = form.querySelector('input');
        if (input) input.value = '';
      });
    }
  }

  if (document.readyState === 'loading') document.addEventListener('DOMContentLoaded', wire);
  else wire();
})();
