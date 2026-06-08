# 5.2 Reporting And Provenance

Scientific reporting in `patina` should make it possible to answer:

1. what structure or state was evaluated
2. which backend or surrogate produced the score
3. which transformation or normalization steps were applied
4. how duplicate handling or acceptance decisions were made

## Reporting Contract

The public documentation should converge on a reporting contract with the following ingredients:

- stable run identifiers
- explicit environment and backend metadata
- named input artifacts
- named output artifacts
- clear distinction between observed values and inferred values

> [!IMPORTANT]
> Good provenance language:
>
> "Candidate `n24_015` was relaxed with backend `gulp`, using atom specification source `atoms.in`,
> with duplicate classification enabled via native-compatible hashkey plus PMOI fallback."

## Why This Matters

Trackability is not bureaucracy. It is what allows a reader to separate:

- chemistry from tooling accident
- a real scientific decision from a default parameter
- a reproducible result from an anecdotal run

The `patina` book should therefore prefer explicit provenance artifacts and citations over narrative
claims that cannot be reconstructed later.
