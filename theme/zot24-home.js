// Inject a "← zot24.com" link into the mdbook topbar so visitors can
// navigate back to the parent site from any page of the docs.
(function () {
  function inject() {
    var bar = document.querySelector('.menu-bar .right-buttons');
    if (!bar) return;
    if (bar.querySelector('.zot24-home-link')) return;

    var a = document.createElement('a');
    a.href = 'https://zot24.com';
    a.className = 'icon-button zot24-home-link';
    a.title = 'Back to zot24.com';
    a.setAttribute('aria-label', 'Back to zot24.com');
    a.rel = 'noopener';
    a.innerHTML = '<span aria-hidden="true">←</span><span class="zot24-home-label">zot24.com</span>';

    bar.insertBefore(a, bar.firstChild);
  }

  if (document.readyState === 'loading') {
    document.addEventListener('DOMContentLoaded', inject);
  } else {
    inject();
  }
})();
