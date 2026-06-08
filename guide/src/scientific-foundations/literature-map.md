# 4.3 Literature Map

<p class="lead">
  This map turns the literature into an editing plan for the public book. It is not a bibliography
  dump; it is a guide to which papers should support which claims.
</p>

## Reader Contract

This page explains:

- which papers anchor the public `patina` scientific narrative
- which method families each paper supports
- where future editors should add detail

It does not claim that every paper listed here is already fully represented in the implementation.

## Reading Ladder

| Layer | Start with | Why it matters here |
| --- | --- | --- |
| Energy landscape search | @wales1997, @goedecker2004 | Explains transformed landscapes, local minima, and why perturb-relax loops matter. |
| Cluster genetic algorithms | @deaven1995, @lazauskas2017ga | Explains population search, crossover, mutation, duplicate pressure, and nanoscale cluster search. |
| Crystal structure prediction | @woodley1999ga, @woodley2008csp, @oganov2006jcp, @glass2006uspex | Gives the broader CSP context and the evolutionary-search lineage. |
| Surface and extended materials | @deaconsmith2014surface | Keeps surface optimisation visible without presenting current surface reconstruction as complete. |
| Structure identity and descriptors | @bartok2013soap, @deringer2018boron | Separates exact identity, descriptor similarity, local environments, and data-driven structure learning. |
| Machine-learning potentials | @behler2007, @deringer2018csp | Explains why learned potentials and descriptors are useful but must preserve provenance. |
| Active learning | @lookman2019active, @vandermause2020flare, @zhang2020dpgen, @bisbo2022gofee | Gives language for uncertainty, acquisition, concurrent learning, and surrogate-assisted search. |

## Core Search Papers

### Basin Hopping And Minima Hopping

The basin-hopping paper by Wales and Doye is the cleanest anchor for the transformed-landscape
story: perturb a configuration, locally minimize it, then accept or reject the relaxed minimum
@wales1997. That belongs near the basin-hopping chapter, not hidden in architecture notes.

Goedecker's minima-hopping paper is useful as a contrast because it emphasizes rapid movement
through local minima and avoidance of revisiting known regions @goedecker2004. It gives language
for exploration pressure without forcing every search workflow into a genetic-algorithm frame.

### Genetic Algorithms For Clusters

Deaven and Ho are the classic cluster-GA anchor for molecular geometry optimisation @deaven1995.
For `patina`, the more directly relevant paper is Lazauskas, Sokol, and Woodley because it is
explicitly nanoscale-search-facing and makes structure diversity and topological duplicate removal central
@lazauskas2017ga.

This distinction matters for edits:

- use @deaven1995 for the general GA method lineage
- use @lazauskas2017ga for duplicate control, topological analysis, and nanoscale cluster search

### Crystal Structure Prediction

Woodley, Battle, Gale, and Catlow provide the inorganic crystal GA starting point @woodley1999ga.
Woodley and Catlow then give the broader first-principles CSP review frame @woodley2008csp.
Oganov and Glass, and the later USPEX implementation paper by Glass, Oganov, and Hansen, are the
right comparison points for evolutionary CSP beyond the legacy cluster-search line @oganov2006jcp
@glass2006uspex.

This cluster should support the public explanation that `patina` is not only a cluster-search tool.
It sits in a broader family of structure-prediction methods where local relaxation, candidate
generation, and metastable-structure reporting are all method-level concerns.

## Identity, Similarity, And Descriptor Papers

The public book needs a sharper split between exact identity and graded similarity.

@lazauskas2017ga supports the hard duplicate-control story for cluster-style search. @bartok2013soap
supports the descriptor story: useful structural representations should respect invariance to
translation, rotation, reflection, and permutation of like atoms. @deringer2018boron is useful for
showing how learned local and total energy models can become part of structure discovery while
still requiring careful dataset provenance.

The editing rule is:

- exact topology identity belongs in duplicate-control and run-integrity explanations
- descriptors belong in similarity, surrogate modeling, and active-learning explanations
- energy alone should not be presented as a structure identity

## Active-Learning Papers

The active-learning lane should not read like "add ML here." The core claim is more specific:
uncertainty and acquisition policy decide when a cheaper learned model may advise a more expensive
calculation.

Useful anchors:

- @lookman2019active for the broad materials-science active-learning vocabulary
- @vandermause2020flare for on-the-fly Bayesian force-field training with uncertainty-triggered
  first-principles calls
- @zhang2020dpgen for concurrent learning and dataset generation for deep-potential models
- @bisbo2022gofee for global optimisation with first-principles energy expressions accelerated by
  a surrogate model trained during search

For `patina`, these papers support the architecture rule that emulators and active-learning
coordinators should advise or gate exact evaluation through explicit provenance, not silently
replace evaluator truth.

## Where To Edit Next

| Target page | Missing expansion | Best sources |
| --- | --- | --- |
| `genetic-algorithm.md` | explicit duplicate-control and diversity section | @deaven1995, @lazauskas2017ga |
| `basin-hopping.md` | transformed PES and walker-continuity explanation | @wales1997, @goedecker2004 |
| `emulators.md` | uncertainty, acquisition, property targets, and exact-evaluation promotion | @lookman2019active, @vandermause2020flare, @zhang2020dpgen, @bisbo2022gofee |
| `structure-identity.md` | exact topology identity versus descriptor similarity | @lazauskas2017ga, @bartok2013soap |
| future surface page | cautious surface optimisation story | @deaconsmith2014surface |

## Citation Hygiene

When adding a new source:

1. add it to `references.bib`
2. cite it in a paragraph that says why it matters for `patina`
3. avoid copying abstracts into the book
4. prefer one or two high-value citations per paragraph over long citation chains
