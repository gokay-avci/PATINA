# 3. PATINA Overview

`patina` is the new public-facing identity for the Rust workspace.

The central shift is from an operator-local, historically accreted project layout toward a cleaner
workspace with:

- typed crate boundaries
- generic local setup instructions
- explicit provenance for imported scientific logic
- public documentation that is separate from campaign scratch material

This section should become the canonical reference for:

- what the workspace is
- how a contributor gets it running
- which crates are public API candidates
- which scientific assumptions are stable enough to document as contracts

The name `patina` is intentionally larger than a crate rename exercise. It is the boundary where we
stop treating private machine state, legacy sibling repositories, and ad hoc execution notes as
part of the public contract.
