# 2. Introduction

This book serves two overlapping audiences and one practical campaign at the same time:

- readers who want the scientific logic of global optimisation
- readers who want the software architecture that keeps those workflows portable and auditable
- contributors preparing `patina` for a cleaner public release surface

## Reading Order

For most readers, the intended order is:

1. [3. PATINA Overview](./patina/index.html)
2. [4. Scientific Foundations](./scientific-foundations.html)
3. [5. Computational Chemistry](./computational-chemistry/index.html)
4. [6. Software Architecture](./software-architecture/index.html)
5. [7. Legacy KLMC3](./legacy-klmc3/index.html) only when migration context matters

## Scientific Readers

Start with [4. Scientific Foundations](./scientific-foundations.html), then continue to
[5. Computational Chemistry](./computational-chemistry/index.html).

Focus:

- search logic
- provenance
- interpretation
- scientific trust

## Software Readers

Start with [3. PATINA Overview](./patina/index.html), then continue to
[6. Software Architecture](./software-architecture/index.html).

Focus:

- architecture
- environment setup
- interfaces
- reproducibility

## How The Book Is Organized

- `4. Scientific Foundations` explains the durable method logic and reporting philosophy.
- `5. Computational Chemistry` explains structure meaning and provenance expectations.
- `6. Software Architecture` explains the stable engineering contracts, external-tool boundaries,
  and release-facing development surfaces.
- `7. Legacy KLMC3` exists for ancestry and migration context, not as the default operating manual.

## Current Strategy

The documentation program now has three priorities:

1. keep the public book simple enough to serve with `just docs-serve`
2. deepen the scientific foundations with stronger literature coverage
3. document stable architectural meaning without overcommitting to transient crate layout
