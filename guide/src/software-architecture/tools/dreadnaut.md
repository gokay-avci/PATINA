# 6.6.2 Dreadnaut And Topology Identity

<p class="lead">
  `patina-dreadnaut` is the exact topology-identity lane in `patina`: it turns a species-aware graph
  representation of a structure into a canonical label that can be used for native-compatible
  hashkeys, duplicate control, and topology-aware reporting.
</p>

## At A Glance

- Scientific role:
  Collapse true topology duplicates. This lane is for exact identity, not fuzzy similarity.
- Current ownership:
  The crate builds the graph, writes Dreadnaut text, invokes the canonicalizer, and extracts the
  final canonical label as the hashkey.
- Status:
  Keep exact hashkeys here and place partial-topology similarity in descriptor space rather than in
  string matching tricks.

## Stable Idea

The durable concept is:

- build a species-aware radius graph
- canonicalize it with the Dreadnaut / nauty machinery
- use the canonical label as an exact topology identity

That identity is already meaningful in duplicate control, solid-solution reporting, and parity-aware
workflow decisions.

{{#tabs global="lens"}}
{{#tab name="Materials Lens"}}

For scientific reporting, exact topology identity helps distinguish:

- the same connectivity reached more than once
- a genuinely new local minimum with different graph structure
- a near miss that should be studied as a related neighborhood, not mislabeled as identical

{{#endtab}}
{{#tab name="Software Lens"}}

For software architecture, the key boundary is explicit:

- graph building lives in Rust
- canonicalization is treated as a dedicated adapter path
- the hashkey remains an exact identity artifact, not a generic similarity metric

{{#endtab}}
{{#tab name="Bridge"}}

The bridge rule is important: exact identity and graded similarity are both useful, but they
should not be forced into the same object.

{{#endtab}}
{{#endtabs}}

## Boundary

Owned today:

- graph construction from candidate geometry
- graph export in Dreadnaut format
- canonical hashkey extraction
- topology-margin and edit-style helpers that support sensitivity analysis

Explicitly not the right use:

- partial string matching on canonical labels as a scientific similarity measure
- pretending exact canonical labels are enough to represent landscape neighborhoods by themselves
