# 4.2.1 Current Emulator Prototypes

<p class="lead">
  This page records what exists today so future edits can separate working infrastructure, partial
  demonstrations, and research intentions.
</p>

## Reader Contract

This page is an implementation-facing inventory for the emulator lane. It is meant to be edited as
the prototypes mature.

It answers four questions:

- what can currently be pointed to in the repository
- which artifacts prove the workflow shape
- which pieces are still research-only
- what should become a public tutorial later

## Implemented Building Blocks

| Area | Current artifact | Status |
| --- | --- | --- |
| Rust contract crate | `crates/patina-emulate` | implemented |
| Python runtime | `crates/patina-emulate/python` | implemented |
| Environment policy | `venvs/autoemulate`, `just setup-emulate`, `just doctor-python-envs` | implemented |
| Feature projection ports | `FeatureProjectionPort` and projector builders | implemented |
| Rust-native descriptors | `simple_structure_statistics`, `pair_distance_signature` | implemented |
| Python SOAP descriptors | `dscribe_soap`, `featomic_soap` | implemented as Python-backed projectors |
| Surrogate request schema | `patina.emulate.surrogate_request.v1` | implemented |
| Surrogate response schema | `patina.emulate.surrogate_response.v1` | implemented |
| Downstream GA scoring | `patina-driver score-ga-emulate` | implemented path |
| Research uncertainty gate | `run-ga scott-staged --emulate-uncertainty-gate` | implemented but research-only |
| Preserved analysis report | `extra/runs/analysis/mgo12_hybrid_emulate_report/REPORT.md` | partial evidence |

## Rust-Side Contracts

The important Rust types are not model-specific. They describe the scientific traffic:

- `CampaignId` and `BranchId` keep long-lived advisory state addressable.
- `ScientificObjective` names the campaign intent.
- `FidelityClass` records the source of property labels.
- `FeatureRepresentation` names the descriptor family, version, features, and provenance label.
- `PredictionTarget` names the property, head kind, fidelity, and unit.
- `CandidateObservation` stores exact evidence admitted to the learning lane.
- `AcquisitionRecord` carries rank, objective, acquisition score, novelty score, uncertainty
  score, and optional expected improvement.

This is the right abstraction level for public docs. It lets the book explain property learning
without binding the scientific story to one Python class.

## Python Runtime

The Python runtime is deliberately isolated from Janus, GULP, and SCOTT details.

It receives:

- normalized feature matrices
- target values
- pending candidate feature rows
- selected surrogate configuration
- campaign and branch metadata

It returns:

- predicted means
- predicted variances
- acquisition scores
- uncertainty scores
- selected model name
- optional checkpoint path
- ranked candidate order

The local runtime currently builds an AutoEmulate object around the selected SVGP model class. For
small early histories it uses an explicit non-empty evaluation split rather than relying on an
implicit random split. That matters because early GA branches can have very few unique training
observations.

## Feature Projector Status

| Projector | Use now | Good tutorial use |
| --- | --- | --- |
| `simple-structure-statistics` | sanity and small examples | introduce the contract with readable features |
| `pair-distance-signature` | default stronger Rust-native projector | first practical cluster tutorial |
| `dscribe-soap` | descriptor-rich Python path | advanced tutorial on local environments |
| `featomic-soap` | descriptor-rich Python path | comparison against DScribe SOAP |

The public tutorial should probably start with `pair-distance-signature`. It avoids introducing
heavy descriptor dependencies at the same time as AutoEmulate, but it is more chemically meaningful
than a tiny statistics vector.

## Downstream Scoring Prototype

`score-ga-emulate` is the cleanest public shape for the first tutorial.

Conceptually it does this:

1. Read preserved GA generation states from `raw/generation_XXXX_state.json`.
2. Deduplicate evaluated structures for training.
3. Project training and pending candidates into the same feature space.
4. Build a surrogate request.
5. Call the isolated Python runtime.
6. Validate the response against the request.
7. Emit acquisition records for ranked candidates.

The tutorial version should show the artifact tree rather than only a command:

```text
emulate-output/
  request.json
  response.json
  ...
```

The important teaching point is that `request.json` is an auditable statement of the data and
policy passed into the surrogate, while `response.json` is the model's advisory answer.

## Research Uncertainty Gate

The uncertainty gate is more experimental than downstream scoring.

It wraps an exact staged evaluator and chooses between:

- exact warmup
- exact evaluation because there are too few training observations
- exact evaluation because surrogate uncertainty is too high
- exact fallback if scoring fails
- emulated prediction if uncertainty is below the configured threshold

This lane is scientifically useful, but it should stay clearly marked as research-only until the
book has preserved comparisons that show when it helps and when it distorts search behavior.

The artifacts to expose in a future tutorial are:

- `raw/emulate_gate_summary.json`
- `raw/emulate_gate_decision_trace.json`
- `raw/emulate_gate_batch_trace.json`

## Preserved Evidence So Far

The `(MgO)12` hybrid emulate report is a useful partial case study:

- pure GULP GA and hybrid pre-emulate GA reached the same best final energy in the preserved
  comparison
- the hybrid path assembled `1091` training rows and `20` candidate rows for the emulate handoff
- the batch failed before `response.json`
- no acquisition-guided promotion happened in that run

This is not a failed documentation artifact. It is exactly the kind of boundary evidence the public
book should preserve: the exact run completed, the training request was assembled, and the surrogate
runtime boundary was the stopping point.

## Not Yet Done

These are intentionally not claimed as complete:

- a clean public tutorial with a successful `response.json`
- a validated materials-chemistry-community property case study
- parity between emulate-gated search and native SCOTT
- production-quality model selection across descriptor families
- automatic topology-informed numeric descriptors beyond current identity and geometry signals
- multi-target property tutorials using the existing multitask request shape

## Good Next Edits

Good edits from here are concrete:

- add one small successful `score-ga-emulate` run with current `patina-emulate` paths
- record the output artifact tree beside the tutorial
- compare `pair-distance-signature` against one SOAP projector on the same pending set
- add a small table of predicted mean, variance, rank, and exact follow-up energy
- write the first community-facing tutorial around one property and one fidelity label
