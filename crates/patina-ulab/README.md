# patina-ulab

`patina-ulab` is the optional HPC orchestration adapter layer for `patina`.

It sits between:

- `patina-runtime`, which defines scheduler-neutral Scott runtime jobs and reports
- site schedulers and worker allocations, which decide where and when jobs run
- workspace and artifact policy, which decide what lives in durable storage versus scratch

The first concrete terminal external-stage completion bridge now lives in `patina-runtime`.
`patina-ulab` should feed scheduler receipts, launch provenance, and durable prior Scott state into
that report surface rather than re-implement Scott scientific meaning here.

It should own:

- site profiles
- worker capability matching
- work leases
- scheduler adapter traits
- runtime-backed scheduler adapters
- durable receipt/state projection for recovery
- workspace planning
- optional bridge payloads into `unified_lab`-style substrates

It should not own:

- Scott scientific semantics
- DFT deck or parse semantics
- generic workflow-engine logic

Current module split:

- `bridge`: projection of Scott jobs into neutral workflow/worker envelopes
- `site_profile`: scheduler, launcher provenance, scratch, and submission policy for sites such as Archer2 or Young; Young parity is currently modeled from a Grid Engine / shared-scratch `qsub` artifact rather than a Slurm assumption
- `capabilities`: worker resource and tag matching
- `lease`: control-plane lease ownership for staged work
- `reconciliation`: source precedence, recovery reconstruction, and replay-safe operation keys
- `report_bridge`: reconciled conversion from scheduler receipt plus allocation summary into a Scott runtime report
- `scheduler`: scheduler-facing trait plus runtime-backed local, typed Slurm, and typed Grid Engine adapters
- `projection_store`: durable JSON-backed receipt/state store for recovery surfaces
- `workspace`: durable/scratch path planning for one Scott runtime job
