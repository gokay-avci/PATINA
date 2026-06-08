# patina-stk

`patina-stk` is the Rust-native supramolecular topology and construction kernel for this workspace.

Current scope:

- topology graph records and periodic edge semantics
- construction-state and placement kernel
- built-in bond-definition rules for supramolecular assembly
- lightweight optimizer contracts

Current non-goals:

- Python `stk` API parity
- reaction-factory parity
- evolutionary algorithm parity
- MongoDB and other heavy external services

Python ownership:

- this crate owns a small adapter runtime project under `python/`
- that runtime is intended to be installed into the workspace-managed environment `venvs/stk`
- the Rust domain crate must remain independent from RDKit and other Python-native types

Phase 0 status:

- crate boundary established
- module landing zones reserved
- Python adapter landing zone reserved
- Python runtime package and `venvs/stk` bootstrap path implemented
- RDKit runtime status, SMILES canonicalization, SMARTS detection, and conformer embedding implemented
- Rust-side live RDKit integration test implemented
- first `patina-driver` 0D construction/export path implemented
- topology and geometry descriptors implemented for automorphism and internal-coordinate tooling
- STK driver artifacts now include dreadnaut-ready graph text via `patina-dreadnaut`

Parity notes and handoff material are kept out of the public source surface until they are
rewritten as guide pages.
