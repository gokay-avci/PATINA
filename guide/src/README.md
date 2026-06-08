# 1. PATINA Book

> [!NOTE]
> This book is the public, reader-facing documentation surface for `patina`.
> Internal campaign notes, provenance logs, and execution checklists stay under `docs/`.

`patina` is the new public project identity for the Rust workspace in this repository.

<p class="lead">
  This book is written for two audiences at once: readers tracking the scientific meaning of
  structure search, and readers tracking the software architecture that keeps those workflows
  reproducible, auditable, and portable.
</p>

## Book Structure

### [2. Introduction](introduction.md)

The entry point for readers who want the reading order, the audience split, and the documentation
strategy in one place.

### [2.1 Editing This Book](editing-guide.md)

The source-editing contract for future documentation work: where pages belong, how citations should
be used, and which checks keep the public book clean.

### [3. PATINA Overview](patina/)

The public identity of the project, release posture, and the boundary between private migration
scaffolding and public contract.

### [4. Scientific Foundations](scientific-foundations.html)

The durable scientific method layer for global optimisation, reporting, and search interpretation.

- Start here if the main question is method lineage or literature.
- Key topics: basin hopping, GA, Monte Carlo search, emulators, AutoEmulate property guidance, and
  the literature map.

### [5. Computational Chemistry](computational-chemistry/)

Energy landscapes, basin hopping, provenance, and structure-search interpretation.

- Start here if the main question is scientific meaning.
- Key topics: structure semantics, identity, similarity, provenance.

### [6. Software Architecture](software-architecture/)

Typed kernels, ports and adapters, runtime boundaries, and reproducible tooling.

- Start here if the main question is architecture and public-release readiness.
- Key topics: hexagonal ports, workspace policy, tooling boundaries, runtime campaign boundaries.

### [7. Legacy KLMC3](legacy-klmc3/)

Historical context without freezing the new public project identity in the past.

- Use this lane for ancestry, migration context, and old runtime assumptions that still affect
  parity or historical report reading.

The separation is deliberate, but the content is not siloed. Basin hopping is both a scientific
search strategy and a software orchestration problem. Hexagonal ports and adapters are documented
under software architecture because that is the stable engineering contract that keeps scientific
logic from being contaminated by external program details.

## Tooling

This book is designed to be built with a Rust-native toolchain:

- `mdbook`
- `mdbook-mermaid`
- `mdbook-bib`
- `mdbook-tabs`

The current book surface already supports:

- runnable Rust code blocks through `mdbook test`
- build-time command output embedding through `mdbook-cmdrun`
- Mermaid diagrams for process and architecture maps
- MathJax equations for scientific exposition
- Plotly-backed interactive plots for lightweight scientific dashboards
- 3Dmol-backed interactive rendering for `.xyz` and `.cif` examples
- tabs for audience-separated explanations
- bibliography-backed references

For safety, `cmdrun` usage in this book should stay limited to curated local helper scripts that we
own in the repository rather than arbitrary ad hoc shell fragments.

The interactive Plotly and 3Dmol panels are deliberately thin wrappers around committed local data.
That keeps the Markdown readable and the scientific examples auditable while still giving readers a
modern browser-native view.

Install the recommended toolchain with:

```bash
just docs-bootstrap
```

Then build or preview the book with:

```bash
just docs-build
just docs-serve
```

## Typography Control

The global font controls now live in one place: the top of `guide/theme/custom.css`.

The main knob is `--patina-type-scale`. Change that single value first if you want the whole book
larger or smaller.

The next chapters establish the reader map, shared scientific/software vocabulary, and the
development contract for `patina`.
