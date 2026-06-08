# 5. Computational Chemistry

This lane is for readers who primarily want to understand:

- what the search is trying to discover
- how structures are transformed and compared
- how results should be reported so that they remain scientifically auditable

The purpose is not to turn the book into a chemistry textbook. The purpose is to explain the
computational-chemistry assumptions behind the software decisions.

This track should stay stable even if specific crates, modules, or command surfaces are refactored.
The chemistry-facing reader should come away with a picture of:

- which kinds of structures the workspace distinguishes
- which identity and similarity claims are exact, graded, or only energetic
- what information may be inferred or lost during conversion
- how evaluations and search decisions are recorded with enough evidence to trust them later

## Entry Points

- [Structure Semantics](structure-semantics.html):
  dimensionality, periodicity, validated structures, and conversion policy.
- [Structure Identity And Similarity](structure-identity.html):
  exact duplicate gates, topology identity, descriptor similarity, and energy reporting.
- [Reporting And Provenance](reporting-provenance.html):
  how claims become auditable run artifacts.
