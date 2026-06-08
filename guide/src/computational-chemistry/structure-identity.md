# 5.2 Structure Identity And Similarity

<p class="lead">
  Structure identity is not one question. <code>patina</code> needs exact duplicate gates, graded
  similarity measures, descriptor features, and energy reporting to remain separate.
</p>

## Reader Contract

This page explains the concepts that should stay stable even if the implementation of hashkeys,
descriptors, or duplicate policy changes.

It does not claim that every identity mode is production complete in every workflow.

## Four Different Questions

| Question | Example answer | Use with care because |
| --- | --- | --- |
| Are these exactly the same for a duplicate gate? | topology hashkey, canonical graph identity | exact graph identity depends on graph construction policy |
| Are these geometrically close? | RMSD, distance matrix, assignment distance | close geometry can still differ chemically or topologically |
| Are these chemically similar environments? | SOAP-like descriptors, local environment fingerprints | descriptors are graded features, not hard identity proofs |
| Are these energetically equivalent? | same relaxed energy within tolerance | energy degeneracy does not imply structural identity |

## Exact Identity

Exact identity is useful when a workflow must decide whether a candidate should be rejected,
repopulated, or reported as rediscovery.

The nanoscale GA literature makes duplicate control central to efficient search, not a
late reporting convenience @lazauskas2017ga. That is why `patina` should keep topology identity and
duplicate policy visible in run artifacts.

The important public rule is:

- exact identity is a workflow decision
- the graph or hashkey construction policy must be recorded
- failure to compute a required identity should fail loudly, not silently classify a candidate

## Graded Similarity

Similarity is different from identity. A graded similarity can help answer:

- is this candidate near a known motif?
- did mutation preserve the local environment?
- does a surrogate model see this structure as familiar or novel?
- should active learning spend exact evaluation on this candidate?

SOAP-style descriptors are a useful reference point because the descriptor discussion explicitly
centers invariance to translation, rotation, reflection, and permutation of like atoms
@bartok2013soap. Those invariances are not presentation details; they define whether a descriptor
can support meaningful comparison.

## Descriptor Features For Learning

Descriptors can also feed learned potentials or advisory models.

The data-driven boron work by Deringer, Pickard, and Csanyi is a useful example of coupling
machine-learned potentials with structure search while preserving a dataset-generation story
@deringer2018boron. GOFEE gives another useful frame: surrogate relaxations and acquisition
functions can accelerate global optimisation, but the exact-evaluation boundary remains part of
the method @bisbo2022gofee.

For `patina`, that means:

- descriptors may guide search
- descriptors may support active learning
- descriptors should not silently override exact evaluator evidence
- uncertainty and model version should be reportable when a learned model influences a decision

## Reporting Contract

Every structure-comparison artifact should say which layer it belongs to:

| Artifact | Report as |
| --- | --- |
| topology hashkey | exact identity evidence under a stated graph policy |
| graph edit or topology margin | topology sensitivity evidence |
| RMSD or distance matrix score | geometric similarity evidence |
| SOAP or environment fingerprint distance | descriptor-space similarity evidence |
| energy difference | evaluator result, not identity by itself |

## Good Next Edits

The next expansion should add:

- one small diagram showing hard gates versus soft descriptors
- one example of two structures with similar energy but different identity
- one example of an identity failure mode caused by missing external topology tooling
- one paragraph on how active-learning acquisition uses novelty without treating novelty as truth
