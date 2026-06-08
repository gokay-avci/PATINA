# 2.1 Editing This Book

This page is the editing contract for the public mdBook source.

Use it before adding or rewriting chapters. The goal is not to freeze the book. The goal is to
make future edits easier to place, review, and trust.

## Source Folders

| Folder | Purpose | Edit rule |
| --- | --- | --- |
| `guide/src/scientific-foundations/` | Durable algorithms, equations, and literature anchors | Prefer method concepts over crate names. |
| `guide/src/computational-chemistry/` | Structure meaning, identity, provenance, and interpretation | Explain scientific assumptions and reporting consequences. |
| `guide/src/software-architecture/` | Ports, adapters, runtime boundaries, and workspace shape | Explain stable contracts, not every internal module. |
| `guide/src/legacy-klmc3/` | Historical semantics and migration context | Preserve ancestry without making KLMC3 the public operating manual. |
| `guide/src/assets/` | Small committed examples and figures | Keep examples inspectable and repo-owned. |

Generated output under `guide/book/` is not source documentation.

## Page Template

New pages should usually start with this shape:

```markdown
# Numbered Title

<p class="lead">
  One short paragraph explaining why this page exists.
</p>

## Reader Contract

- what the page explains
- what it deliberately does not claim
- which evidence or literature it depends on

## Stable Ideas

Explain concepts that should survive refactors.

## PATINA Reading

Explain how the concept maps onto the current project without overpromising final crate layout.

## Open Edits

- specific next subsection to add
- citation or artifact that would make the page stronger
```

## Citation Rules

- Cite a paper when the page makes a scientific-method claim.
- Cite project docs or artifacts when the page makes a local implementation or provenance claim.
- Do not cite a paper only as decoration. Each citation should explain why the paper matters for
  `patina`.
- Keep broad reviews close to roadmap pages and primary method papers close to algorithm pages.

Good citation use:

- basin-hopping method mechanics near @wales1997
- nanoscale KLMC duplicate-control discussion near @lazauskas2017ga
- active-learning uncertainty language near @lookman2019active and @vandermause2020flare

## Build Checks

Use these from the repository root:

```bash
just docs-build
just docs-serve
```

Before committing a larger documentation pass, also check:

```bash
rg -n "/Users|Desktop|/home/" guide/src docs/active docs/reference/README.md
```

Site-specific paths can exist as provenance in internal reference notes, but they should not leak
into the public book as default instructions.

## Style Rules

- Prefer short sections with explicit reader purpose.
- Use tables for comparisons, not long prose blocks.
- Keep equations near the paragraph that interprets them.
- Use Mermaid only when the diagram clarifies a workflow boundary.
- Avoid marketing language. The book should read like scientific engineering documentation.
- Avoid promising final APIs when the current goal is public clarity and editability.

## Good Next Edits

The next useful manual edits are:

- expand the search literature map with one paragraph per source
- add a surface/global-optimisation page under computational chemistry
- add a short DFT/evaluator hierarchy page once CP2K, GULP, and future VASP/AIMS roles settle
- add one small figure per scientific-foundations subsection
- add public installation instructions that do not depend on private local paths
