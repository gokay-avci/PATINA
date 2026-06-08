# 4.2.2 AutoEmulate Materials Tutorial Seed

<p class="lead">
  This is the starting point for a future tutorial that introduces AutoEmulate-backed property
  guidance to materials-chemistry users.
</p>

## Reader Contract

This page is deliberately written as a tutorial seed rather than a finished tutorial. It gives the
storyline, vocabulary, command shape, and reporting contract that can be fine tuned once a small
successful run is selected.

The tutorial should teach one idea clearly:

> An emulator is a property triage tool. It helps decide which structures deserve exact evaluation
> next; it is not the source of final scientific truth.

## Intended Audience

The intended reader is a materials chemist who may know structure search and property prediction,
but may not care about Rust crates or Python environment boundaries.

The tutorial should therefore lead with:

- the material system
- the property target
- the exact method used for labels
- the candidate pool
- the uncertainty-aware selection rule
- the exact follow-up results

Only after that should it explain the software path.

## Tutorial Storyboard

### 1. Choose The Scientific Question

Start with one property and one material family.

Good first examples:

| Example | Why it works |
| --- | --- |
| `(MgO)N` cluster energy | already aligned with preserved GA runs and current `energy` targets |
| relative stability of generated clusters | easy to explain with exact follow-up |
| formation-energy-like scalar target | natural next step once a consistent label pipeline exists |
| small multi-target property bundle | useful later for multitask SVGP, but not the first tutorial |

The first public tutorial should probably use energy as the target, then explain how the same
contract generalizes to other scalar properties.

### 2. Prepare Exact Observations

The emulator needs labeled examples. In `patina`, those should come from preserved exact or
reference evaluations, not from an untracked spreadsheet.

Minimum observation record:

| Field | Meaning |
| --- | --- |
| candidate id | stable row identifier |
| structure | the atoms and coordinates being described |
| target value | the property label, for example energy |
| fidelity | the source of the label, for example `gulp-reference` |
| provenance | run, generation, source label, and descriptor provenance |

For current GA runs, the source is the preserved generation-state files:

```text
runs/active/<ga-run>/raw/generation_0000_state.json
runs/active/<ga-run>/raw/generation_0001_state.json
...
```

### 3. Choose Pending Candidates

The tutorial should keep pending candidates small and inspectable at first.

Acceptable current formats include:

- `.json`
- `.xyz`
- `.extxyz`
- `.cif`
- `.car`
- `.arc`
- `.can`

The pending set should be described as "candidates to triage," not "structures already validated."

### 4. Pick A Feature Projector

Start simple:

```text
--feature-projector pair-distance-signature
```

This is the best first tutorial default because it is Rust-native and more informative than a tiny
statistics vector.

Then explain the advanced options:

| Projector | Tutorial position |
| --- | --- |
| `simple-structure-statistics` | smoke test and teaching aid |
| `pair-distance-signature` | first practical example |
| `dscribe-soap` | descriptor-rich follow-up |
| `featomic-soap` | descriptor-rich comparison path |

SOAP descriptors belong in a later section because they introduce a separate lesson about local
environment representations and symmetry-respecting descriptors @bartok2013soap.

### 5. Fit The AutoEmulate-Backed Surrogate

The current runtime uses AutoEmulate around configured SVGP model classes. For a first tutorial,
use the default single-target path:

```text
--model-variant whitened-svgp
--surrogate-objective minimize
--target-name energy
--target-unit eV
```

The useful explanation is:

- `whitened-svgp` is the surrogate family
- `minimize` means lower target values are better
- `energy` is the property label
- `eV` makes predictions readable
- the output includes both predicted mean and uncertainty

Avoid presenting `epochs`, `num-inducing`, and learning rate as chemistry knobs. They are modeling
knobs and should be introduced only after the first run is understandable.

### 6. Run The Current Command Shape

The current command shape is:

```bash
just setup-emulate
just doctor-python-envs

cargo run -p patina-driver -- score-ga-emulate \
  --ga-run-dir runs/active/<ga-run> \
  --output-dir runs/active/<emulate-output> \
  --pending-candidate-dir scratch/<pending-candidates> \
  --campaign-id campaign-mgo-demo \
  --branch-id branch-ga-1 \
  --objective reduce-uncertainty \
  --fidelity gulp-reference \
  --feature-projector pair-distance-signature \
  --target-name energy \
  --target-unit eV \
  --model-variant whitened-svgp \
  --surrogate-objective minimize \
  --emulate-environment autoemulate
```

This is a template, not a guaranteed completed example. The tutorial should replace the placeholder
paths with a checked run and a checked pending directory.

### 7. Read The Output As A Materials Chemist

The important columns in the response are:

| Output | Meaning |
| --- | --- |
| `candidate_id` | which pending structure was scored |
| `means` | predicted property value |
| `variances` | predictive variance for the target |
| `acquisition_score` | ranking score used by the advisory policy |
| `uncertainty_score` | how much the model distrusts the prediction |
| `rank` | advisory order, not final scientific ordering |

The tutorial should include a table like this:

| Rank | Candidate | Predicted energy | Uncertainty | Exact follow-up | Interpretation |
| --- | --- | --- | --- | --- | --- |
| 1 | `cand-a` | fill later | fill later | fill later | selected for exact check |
| 2 | `cand-b` | fill later | fill later | fill later | lower priority or uncertainty probe |

The exact follow-up column is the point of the tutorial. It prevents readers from treating the
emulator ranking as the final result.

### 8. Promote Candidates To Exact Evaluation

The public story should be:

- use the emulator to choose candidates
- evaluate selected candidates exactly
- compare prediction to exact result
- append exact results back into the evidence pool
- repeat only if the provenance remains readable

This aligns with active-learning practice: uncertainty is not just an error bar for a plot; it is a
decision signal for where the next expensive calculation should go @lookman2019active
@vandermause2020flare.

## Research Gate Sidebar

The `--emulate-uncertainty-gate` path is more advanced than the first tutorial.

It can record:

- exact warmup
- exact insufficient training
- exact high uncertainty
- exact fallback
- emulated prediction

This is valuable for research, but it is not the first community-facing workflow. The first tutorial
should use downstream scoring and exact promotion because that keeps the scientific boundary easier
to audit.

## How To Present This To The Community

Use this language:

- "The emulator ranks candidates for follow-up."
- "Uncertainty is part of the decision, not an afterthought."
- "The exact evaluator remains the source of final labels."
- "Every learned decision has a request and response artifact."
- "Feature provenance and target fidelity are reported with the prediction."

Avoid this language:

- "The emulator replaces the evaluator."
- "The model found the ground state."
- "The ranking is the result."
- "The uncertainty score is a calibrated universal error bar."
- "The descriptor choice is a technical detail."

## Minimum Finished Tutorial Checklist

Before this becomes a polished public tutorial, add:

- one named run directory
- one named pending-candidate directory
- the exact command that produced `request.json`
- the exact command that produced `response.json`
- a small result table with predictions and exact follow-up
- a note on descriptor family and target fidelity
- a short failure-mode section

The failure-mode section is important. A good community tutorial should explain what happens when
the Python environment is missing, when too few training rows are available, when a SOAP dependency
fails, or when uncertainty is too high to justify a shortcut.
