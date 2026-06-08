(() => {
    function unwrapHeaderAnchors() {
        const selectors = [
            '.content main h1 > a.header',
            '.content main h2 > a.header',
            '.content main h3 > a.header',
            '.content main h4 > a.header',
            '.content main h5 > a.header',
            '.content main h6 > a.header'
        ];

        document.querySelectorAll(selectors.join(', ')).forEach((anchor) => {
            const heading = anchor.parentElement;
            if (!heading) {
                return;
            }

            while (anchor.firstChild) {
                heading.insertBefore(anchor.firstChild, anchor);
            }
            anchor.remove();
        });
    }

    if (document.readyState === 'loading') {
        document.addEventListener('DOMContentLoaded', unwrapHeaderAnchors, { once: true });
    } else {
        unwrapHeaderAnchors();
    }
})();
