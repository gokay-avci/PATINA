// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

(() => {
    const darkThemes = ['ayu', 'navy', 'coal'];
    const lightThemes = ['light', 'rust'];

    const classList = document.getElementsByTagName('html')[0].classList;
    const rootStyles = getComputedStyle(document.documentElement);

    let lastThemeWasLight = true;
    for (const cssClass of classList) {
        if (darkThemes.includes(cssClass)) {
            lastThemeWasLight = false;
            break;
        }
    }

    const mermaidFontFamily =
        rootStyles.getPropertyValue('--patina-font-heading-family').trim() ||
        'Avenir Next, Segoe UI, Noto Sans, Helvetica Neue, sans-serif';
    const mermaidFontSize =
        rootStyles.getPropertyValue('--patina-mermaid-font-size').trim() || '18px';

    const theme = lastThemeWasLight ? 'base' : 'base';
    const themeVariables = lastThemeWasLight
        ? {
            fontFamily: mermaidFontFamily,
            fontSize: mermaidFontSize,
            primaryColor: '#ffffff',
            primaryBorderColor: '#6d7a82',
            primaryTextColor: '#20303a',
            secondaryColor: '#ffffff',
            secondaryBorderColor: '#6d7a82',
            tertiaryColor: '#ffffff',
            tertiaryBorderColor: '#6d7a82',
            lineColor: '#596871',
            clusterBkg: '#ffffff',
            clusterBorder: '#a2adb3',
            edgeLabelBackground: '#ffffff'
        }
        : {
            fontFamily: mermaidFontFamily,
            fontSize: mermaidFontSize,
            primaryColor: '#162026',
            primaryBorderColor: '#9eacb3',
            primaryTextColor: '#e8e1d6',
            secondaryColor: '#162026',
            secondaryBorderColor: '#9eacb3',
            tertiaryColor: '#162026',
            tertiaryBorderColor: '#9eacb3',
            lineColor: '#d8d2c8',
            clusterBkg: '#162026',
            clusterBorder: '#7f9098',
            edgeLabelBackground: '#162026'
        };

    mermaid.initialize({
        startOnLoad: true,
        theme,
        themeVariables,
        flowchart: {
            useMaxWidth: true,
            htmlLabels: true,
            curve: 'linear',
            nodeSpacing: 56,
            rankSpacing: 78,
            padding: 18
        }
    });

    // Simplest way to make mermaid re-render the diagrams in the new theme is via refreshing the page

    for (const darkTheme of darkThemes) {
        document.getElementById(darkTheme).addEventListener('click', () => {
            if (lastThemeWasLight) {
                window.location.reload();
            }
        });
    }

    for (const lightTheme of lightThemes) {
        document.getElementById(lightTheme).addEventListener('click', () => {
            if (!lastThemeWasLight) {
                window.location.reload();
            }
        });
    }
})();
