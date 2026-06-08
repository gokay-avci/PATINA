# 4.2 Emulators

<p class="lead">
  The emulator lane in <code>patina</code> is the bridge between exact structure-search evidence and
  learned materials-property guidance. It exists to help decide where expensive evaluation should be
  spent, while keeping the scientific meaning of exact workflows intact.
</p>

## Reader Contract

This section explains:

- what an emulator means in this project
- what has already been prototyped
- how AutoEmulate fits into a materials-chemistry property workflow
- which claims remain advisory rather than parity claims
- how this lane can grow into tutorial material for the wider materials-chemistry community

It does not claim that learned predictions are a replacement for GULP, Janus/MACE, DFT, or native
SCOTT evidence.

## Core Idea

An emulator is not just "a faster evaluator." In this project it is a learned advisory system that
tries to rank, score, or prioritize candidates before expensive exact evaluation is spent on them.

That creates two obligations:

- keep the advisory lane scientifically legible
- keep it architecturally separated from deterministic parity-core workflows

The scientific language comes from active learning and surrogate-assisted structure search:
uncertainty and acquisition policy decide when a candidate deserves exact follow-up
@lookman2019active @vandermause2020flare @zhang2020dpgen @bisbo2022gofee. In `patina`, that maps to
a strict rule: exact evaluators create scientific evidence, while emulators create advice about
which evidence to buy next.

## Workflow Shape

The current design can be read as a six-stage loop:

1. A search controller proposes and evaluates structures through an exact or reference backend.
2. Accepted evaluated structures become training observations.
3. A feature projector turns structures into emulator-ready descriptors.
4. The Python AutoEmulate runtime fits an uncertainty-aware surrogate.
5. Pending candidates are scored and ranked with predicted means, variances, acquisition scores,
   and uncertainty scores.
6. The driver either promotes candidates for exact evaluation or, in the research-only gate, uses
   low-uncertainty predictions under explicit provenance.

That loop is deliberately asymmetric. Information can flow from exact evaluation into the emulator,
and emulator advice can influence scheduling, but emulator output should not silently rewrite the
meaning of exact evaluation records.

## What Has Been Implemented So Far

The current `patina-emulate` story is already more structured than a vague "ML hook":

- Rust contracts for campaigns, branches, feature representations, prediction targets, fidelity
  classes, candidate observations, and acquisition records
- typed surrogate request and response schemas under `patina.emulate.surrogate_request.v1` and
  `patina.emulate.surrogate_response.v1`
- a Python runtime owned under `crates/patina-emulate/python`
- explicit environment management through `venvs/autoemulate`
- `score-ga-emulate` support for downstream scoring of pending candidates from preserved GA
  generation states
- a research-lane `--emulate-uncertainty-gate` that keeps exact evaluation underneath and records
  whether each candidate was exact warmup, exact fallback, high-uncertainty exact evaluation, or an
  emulated prediction
- request artifacts written as `request.json` and `response.json` inside surrogate batch
  directories

The current prototypes are inventoried in
[4.2.1 Current Emulator Prototypes](emulators/current-prototypes.md).

## Feature Families Explored

| Family | Current role | Good for | Main caution |
| --- | --- | --- | --- |
| `simple_structure_statistics.v1` | lightweight Rust-native descriptor | smoke tests, small examples, shape validation | too coarse for serious chemistry claims |
| `pair_distance_signature.v1` | stronger Rust-native geometry signature | cluster triage and simple similarity-aware scoring | not a full local-environment descriptor |
| `dscribe_soap` | Python-backed SOAP projection | richer local-environment information | heavier dependency path |
| `featomic_soap` | Python-backed SOAP projection | alternative SOAP implementation path | heavier dependency path |

These correspond to different bets about what kind of structure information is useful to a
surrogate:

- very lightweight geometry summaries
- richer pair-distance signatures
- SOAP-style descriptors through Python-backed tooling

SOAP is important because useful atomistic descriptors must respect the physical symmetries of the
problem, including translation, rotation, and permutation of like atoms @bartok2013soap. That is why
the emulator lane should talk about descriptor provenance, not just "features."

There is also a clear local design direction toward topology-informed features rather than only
exact topology identity. The existing project notes argue that exact `dreadnaut` hashkeys should
stay binary identity gates, while graded graph descriptors should become numeric surrogate inputs.

## Model Families Explored

| Variant | Current use | Why it matters |
| --- | --- | --- |
| `mean-field-svgp` | single-target surrogate option | simpler sparse variational Gaussian-process baseline |
| `unwhitened-svgp` | single-target surrogate option | GP-flavoured uncertainty with a different parameterization |
| `whitened-svgp` | default single-target option | current flagship choice for advisory scoring |
| `multitask-mean-field-svgp` | multi-target option | future property bundles with several scalar heads |
| `multitask-unwhitened-svgp` | multi-target option | multi-property uncertainty path |
| `multitask-whitened-svgp` | multi-target option | intended high-value path for property tutorials |

That is a sensible current posture. The repo has repeatedly converged on SVGP as the flagship
advisory family because it keeps uncertainty visible instead of pretending ranking confidence is
free.

AutoEmulate is used here as the Python-facing orchestration layer for fitting and comparing the
surrogate model class selected by the Rust request. The local runtime currently asks AutoEmulate to
fit the configured SVGP model, report evaluation metrics such as `r2` and `rmse`, expose a fitted
predictor, and return means and variances for pending candidates.

## Property Targets

For a materials-chemistry audience, the most important tutorial shift is from "predict energy" to
"predict a named property under a named fidelity." The current code already has the contract needed
for that shift:

| Contract field | Meaning |
| --- | --- |
| `target_name` | the property being learned, for example `energy`, `formation_energy_ev`, or a future property label |
| `target_unit` | the unit that makes predictions readable, commonly `eV` for current energy examples |
| `head_kind` | whether the prediction head is scalar, vector, forces, or multi-head scalar |
| `fidelity` | where the observation came from, such as `gulp_reference`, `janus_mace_low`, `janus_mace_high`, or `reference_dft` |
| `primary_target_index` | which target drives acquisition when several properties are present |

This is the right starting point for community tutorials. A tutorial should not begin with model
internals; it should begin with a property ledger:

- what property is being learned
- which exact method produced the training labels
- which structures are in the training set
- which structures are pending
- what the emulator is allowed to decide
- what still requires exact evaluation

That phrasing makes the work legible to materials chemists who care about property provenance more
than software layering.

## KLMC3 And PATINA Reading

{{#tabs global="emulator-compare"}}
{{#tab name="KLMC3 Lens"}}

There is no native SCOTT equivalent for this lane. That matters because emulator behavior should
never be back-projected into the meaning of KLMC3 workflows as if it were historically native.

{{#endtab}}
{{#tab name="PATINA Lens"}}

PATINA owns this lane explicitly through `patina-emulate`, `patina-driver` adapters, typed feature
projection, and a Python surrogate runtime. The current direction is downstream advisory scoring,
hybrid promotion support, and uncertainty-aware follow-up decisions rather than full in-loop
replacement of exact evaluation.

{{#endtab}}
{{#tab name="Bridge"}}

The bridge rule is strict: if a candidate ranking or acceptance decision was informed by an
emulator, the provenance should say so plainly. Advisory scoring is useful; silent semantic drift
is not.

{{#endtab}}
{{#endtabs}}

## Advisory Workflows Attempted

From the current local provenance, the emulator work has already explored several concrete ideas:

- advisory replay over preserved GA results
- downstream candidate scoring
- uncertainty-reduction objectives
- hybrid GA promotion with optional emulate-related outputs
- uncertainty-gated research-only routes
- topology-aware feature strategies for future surrogate guidance

The important pattern is that the emulate lane has been treated as a decision-support layer around
search artifacts, not as a substitute for exact backend evaluation.

The clearest preserved report so far is the `(MgO)12` hybrid emulate analysis under
`extra/runs/analysis/mgo12_hybrid_emulate_report/REPORT.md`. It records a useful partial success:
the GA-side evidence was harvested into an emulate request with `1091` training rows and `20`
candidate rows. It also records the current limitation clearly: the surrogate batch failed before
`response.json`, so no acquisition-guided promotion happened in that run. That failure is still a
good tutorial artifact because it shows the exact boundary where the workflow stopped.

## Tutorial Direction

The community-facing tutorial should be written as a reproducible property triage workflow:

1. Start from a preserved GA run with exact energies.
2. Choose a small pending candidate set.
3. Select a feature projector and explain why it is appropriate.
4. Fit the AutoEmulate-backed surrogate.
5. Rank pending candidates by acquisition and uncertainty.
6. Promote selected candidates to exact evaluation.
7. Compare the emulator's predicted ranking with exact follow-up.
8. Report which decisions were advisory and which observations were exact.

The seed page for that tutorial is
[4.2.2 AutoEmulate Materials Tutorial Seed](emulators/autoemulate-materials-tutorial.md).

## Guardrails

The current guardrails are scientifically healthy and should stay visible in the public docs:

- emulate-guided search is research or advisory, not SCOTT parity
- Python-heavy ML dependencies stay outside the Rust scientific core
- feature projection and surrogate scoring should use typed ports and explicit artifacts
- uncertainty and model choice should be reported, not hidden
- emulator decisions should never be smuggled into the deterministic core as if they were evaluator facts
- tutorial examples should include the failed or uncertain cases, not only the clean success path

## What Comes Next

The next good expansion of this section is not another UI layer. It is better scientific reporting
around the advisory lane:

- compare feature families on preserved search data
- turn the `(MgO)12` report into a cleaned public case study once the runtime path is reproducible
- add a successful `score-ga-emulate` walkthrough using current `patina-*` paths
- explain when uncertainty reduction is more useful than direct ranking
- document how hybrid promotion uses emulate advice without erasing the exact evaluation boundary
- make the topology-informed emulator direction scientifically legible
- build one multi-target property example using the existing prediction-target contract

This keeps the emulator story aligned with the larger `patina` project goal: richer search support
without sacrificing trackability.
