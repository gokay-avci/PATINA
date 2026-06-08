(function () {
  const root = window.path_to_root || "";
  const PLOTLY_BUNDLE = `${root}theme/vendor/plotly-3.4.0.min.js`;
  const MOL_BUNDLE = `${root}theme/vendor/3Dmol-2.5.3.min.js`;

  const scriptCache = new Map();

  function loadScriptOnce(src) {
    if (scriptCache.has(src)) {
      return scriptCache.get(src);
    }

    const promise = new Promise((resolve, reject) => {
      const existing = document.querySelector(`script[src="${src}"]`);
      if (existing && existing.dataset.loaded === "true") {
        resolve();
        return;
      }

      const script = existing || document.createElement("script");
      script.src = src;
      script.async = true;

      script.addEventListener("load", () => {
        script.dataset.loaded = "true";
        resolve();
      });
      script.addEventListener("error", () => {
        reject(new Error(`failed to load ${src}`));
      });

      if (!existing) {
        document.head.appendChild(script);
      }
    });

    scriptCache.set(src, promise);
    return promise;
  }

  function fallbackMessage(node, message) {
    node.innerHTML = "";
    const box = document.createElement("div");
    box.className = "interactive-fallback";
    box.textContent = message;
    node.appendChild(box);
  }

  function readInlineScript(id) {
    if (!id) {
      return null;
    }
    const script = document.getElementById(id);
    if (!script) {
      return null;
    }
    return script.textContent;
  }

  async function initPlotlyFigures() {
    const figures = Array.from(document.querySelectorAll(".plotly-figure"));
    if (figures.length === 0) {
      return;
    }

    if (!window.Plotly) {
      try {
        await loadScriptOnce(PLOTLY_BUNDLE);
      } catch (_) {
        figures.forEach((node) => {
          fallbackMessage(
            node,
            "Interactive Plotly figure could not load from the vendored local bundle."
          );
        });
        return;
      }
    }

    if (!window.Plotly) {
      figures.forEach((node) => {
        fallbackMessage(node, "Plotly loaded incorrectly in this browser session.");
      });
      return;
    }

    for (const node of figures) {
      const src = node.dataset.plotlySrc;
      const inline = node.dataset.plotlyInline;
      if (!src && !inline) {
        fallbackMessage(node, "Plotly figure is missing its data source.");
        continue;
      }

      try {
        let spec;
        const inlineRaw = readInlineScript(inline);
        if (inlineRaw) {
          spec = JSON.parse(inlineRaw);
        } else {
          const response = await fetch(src);
          if (!response.ok) {
            throw new Error(`failed to fetch ${src}`);
          }
          spec = await response.json();
        }
        const layout = Object.assign(
          {
            paper_bgcolor: "rgba(0,0,0,0)",
            plot_bgcolor: "rgba(255,255,255,0.88)",
            margin: { l: 64, r: 24, t: 36, b: 52 },
            font: {
              family: 'Avenir Next, Segoe UI, Noto Sans, Helvetica Neue, sans-serif',
              size: 15,
              color: getComputedStyle(document.documentElement)
                .getPropertyValue("--fg")
                .trim() || "#20303a"
            }
          },
          spec.layout || {}
        );
        const config = Object.assign(
          { responsive: true, displaylogo: false },
          spec.config || {}
        );

        await window.Plotly.newPlot(node, spec.data || [], layout, config);
      } catch (_) {
        fallbackMessage(node, "Plotly figure data could not be loaded.");
      }
    }
  }

  function viewerStyleFor(kind) {
    if (kind === "sphere") {
      return { sphere: { scale: 0.34, colorscheme: "Jmol" } };
    }
    return { stick: { radius: 0.16, colorscheme: "Jmol" } };
  }

  async function initMolViewers() {
    const viewers = Array.from(document.querySelectorAll(".mol-viewer"));
    if (viewers.length === 0) {
      return;
    }

    if (!window.$3Dmol) {
      try {
        await loadScriptOnce(MOL_BUNDLE);
      } catch (_) {
        viewers.forEach((node) => {
          fallbackMessage(
            node,
            "Interactive molecular viewer could not load from the vendored local bundle."
          );
        });
        return;
      }
    }

    if (!window.$3Dmol) {
      viewers.forEach((node) => {
        fallbackMessage(node, "3Dmol loaded incorrectly in this browser session.");
      });
      return;
    }

    for (const node of viewers) {
      const src = node.dataset.molSrc;
      const inline = node.dataset.molInline;
      const format = node.dataset.molFormat;
      const style = node.dataset.molStyle || "stick";

      if ((!src && !inline) || !format) {
        fallbackMessage(node, "Molecular viewer is missing its input source.");
        continue;
      }

      try {
        let text;
        const inlineRaw = readInlineScript(inline);
        if (inlineRaw) {
          text = inlineRaw.trim();
        } else {
          const response = await fetch(src);
          if (!response.ok) {
            throw new Error(`failed to fetch ${src}`);
          }
          text = await response.text();
        }
        const viewer = window.$3Dmol.createViewer(node, {
          backgroundColor: "white"
        });

        viewer.addModel(text, format);
        viewer.setStyle({}, viewerStyleFor(style));
        if (format === "cif") {
          viewer.addUnitCell();
        }
        viewer.zoomTo();
        viewer.render();
      } catch (_) {
        fallbackMessage(node, "Structure data could not be rendered.");
      }
    }
  }

  function initInteractiveScience() {
    initPlotlyFigures();
    initMolViewers();
  }

  if (document.readyState === "loading") {
    window.addEventListener("DOMContentLoaded", initInteractiveScience);
  } else {
    initInteractiveScience();
  }
})();
