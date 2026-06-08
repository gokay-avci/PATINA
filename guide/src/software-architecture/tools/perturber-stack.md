# 6.6.4 Goedecker-Inspired Perturber Stack

<p class="lead">
  `patina-perturber` is the structure-distance and perturbation lane in `patina`, carrying forward the
  useful ideas of Goedecker-style fingerprinting while separating validated structure semantics from
  descriptor and distance semantics.
</p>

## At A Glance

- Scientific role:
  Perturbation, fingerprints, and duplicate logic. This lane helps answer whether two structures
  are identical enough to collapse, different enough to preserve, or nearby enough to compare
  systematically.
- Current ownership:
  `patina-sci-kernel` owns validated structure meaning, while `patina-perturber` owns perturbation,
  fingerprint, environment, and assignment-distance behavior.
- Status:
  The crate already owns overlap-matrix fingerprints, environment fingerprints, assignment
  distance, and perturbation helpers, but downstream duplicate policy still needs deliberate
  choices.

## Stable Idea

The stable contract is:

- structures have an explicit validated representation
- descriptors and distances are layered on top of that representation
- perturbation and duplicate policy are scientific choices, not just implementation details

That is why the newer environment-fingerprint work is additive rather than a silent replacement of
older interfaces.

{{#tabs global="lens"}}
{{#tab name="Materials Lens"}}

For scientific interpretation, this lane is valuable because it gives a structured way to compare
candidate motifs, track perturbation sensitivity, and reason about similarity without relying only
on energy.

{{#endtab}}
{{#tab name="Software Lens"}}

For architecture, the key split is ownership:

- structure semantics stay in shared kernel types
- fingerprint descriptors stay in `patina-perturber`
- workflow controllers decide how duplicate policy should use those distances

{{#endtab}}
{{#tab name="Bridge"}}

The bridge is scientific traceability. If a workflow collapses or preserves a structure because of
a fingerprint distance, that decision should remain explainable.

{{#endtab}}
{{#endtabs}}

## Boundary

Owned today:

- overlap-matrix fingerprinting
- environment-overlap fingerprinting for clusters and fully periodic 3D frameworks
- assignment-based fingerprint distance
- perturbation engines and topology-fragility helpers

Still a deliberate design choice rather than a default:

- wiring `patina-perturber` into every shared duplicate seam
- treating one fingerprint family as the universal definition of structural equality
