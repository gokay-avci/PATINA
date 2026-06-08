# 6.4 Workspace Map

The exact crate graph in `patina` is still evolving, so this page should not be read as a frozen
inventory of every future module or package. It is a map of stable responsibilities.

## Stable Groups

The workspace already has a durable split between a few kinds of crates:

- transport and shared records
- scientific semantics and kernels
- workflow and application orchestration
- external-program adapters
- side-science or specialist crates
- workbench and operator interfaces

## Crates That Look Most Like Reusable Libraries

These crates already read like crates that could stand on their own, even if their APIs may still
change:

- `patina-sci-kernel`
  shared scientific structure semantics, transformations, and conversion policy
- `patina-external`
  the adapter boundary for external evaluators and program-specific deck/output handling
- `patina-stk`
  the supramolecular topology and construction kernel
- `patina-syva`
  the Rust-owned 0D cluster symmetry lane
- `patina-dreadnaut`
  topology graph construction and Dreadnaut/hashkey I/O
- `patina-types`
  shared transport records and workflow payloads

These are the crates most likely to become individually legible to outside users.

## Crates That Are More Tied To Full `patina`

Other crates are meaningful, but they are more coupled to the integrated workflow story:

- `patina-driver`
  top-level CLI and workflow composition
- `patina-runner`
  runtime dispatch and worker-pool orchestration
- `patina-search`
  search controllers and SCOTT-shaped workflow logic
- `patina-tui`
  operator-facing terminal interface for runs and seams
- `patina-test`
  workspace-specific fixtures and parity support

These may eventually expose reusable pieces, but they are currently easier to understand as part of
the larger `patina` system than as isolated crates.

## Side-Science And Specialist Lanes

Some crates are important, but they should be described carefully because they represent specialist
lanes rather than the whole identity of the project:

- `patina-surface`
  surface import and analysis lane
- `patina-raspa`
  periodic framework and adsorption-facing lane
- `patina-emulate`
  surrogate-learning and advisory lane
- `patina-llm`
  assistant/provider infrastructure lane

These are real parts of the workspace, but not every user of the project needs them to understand
the central architecture.

## Durable Reading Rule

If future refactors merge, split, or rename crates, this higher-level map should still hold:

- semantics inward
- orchestration around them
- adapters at the edges
- specialist lanes kept explicit instead of being smuggled into the core
