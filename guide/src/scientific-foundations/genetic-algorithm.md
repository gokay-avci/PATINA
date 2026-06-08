# 4.1.1 Genetic Algorithm

<p class="lead">
  The durable idea behind the <code>patina</code> genetic-algorithm lane is not a frozen crossover
  implementation. It is the combination of population search, evaluator-backed scoring, duplicate
  pressure, and generation-level provenance.
</p>

## At A Glance

- Scientific role:
  Search a broad motif space while preserving enough diversity to keep escaping local families.
  Selection pressure should reward low-energy or otherwise favorable candidates, but the reporting
  surface must also show when the population is collapsing onto duplicates.
- Current Rust surface:
  `patina-tools`, `patina-search`, and `patina-driver` already expose a real GA artifact lane. Today
  that includes tracked generation metrics, controller traces, checkpoints, and workflow routes
  such as `run-ga scott-staged` and `run-ga janus-persistent`.
- Documentation rule:
  Document the stable reporting contract now, and avoid over-freezing the exact internal operator
  graph. The method page should emphasize population semantics, artifact semantics, and external
  evaluation boundaries.

## Literature Anchors

For this book, the GA story should be read as a lineage rather than a single implementation:

- @woodley1999ga shows the early inorganic crystal-structure prediction framing: candidate
  generation, screening, and local energy minimisation
- @woodley2008csp places evolutionary search inside the broader structure-prediction landscape
- @lazauskas2017ga is especially relevant to `patina` because it emphasizes Lamarckian relaxation,
  duplicate removal, topological analysis, and diversity-preserving mutation classes

That is the right abstraction level for public `patina` docs. The durable point is not "which exact
crossover function exists today"; the durable point is how population search, local relaxation,
duplicate control, and reporting fit together.

{{#tabs global="ga-lens"}}
{{#tab name="Materials Lens"}}

From the chemistry side, GA is valuable because it can compare many structural motifs under a
common evaluation policy while keeping track of which families keep surviving selection.

{{#endtab}}
{{#tab name="Software Lens"}}

From the software side, GA is valuable because its artifact surface is naturally rich:
generation metrics, controller trace, restart checkpoint, run manifest, and family lineage all
have clear homes.

{{#endtab}}
{{#tab name="Bridge"}}

The bridge is that scientific trust often depends less on the crossover slogan and more on whether
the run exposes enough evidence to explain why a motif dominated the population.

{{#endtab}}
{{#endtabs}}

## A Useful Explanatory Equation

For explanation purposes, it is often clearer to write selection in terms of weights and
probabilities:

$$
w_i = \exp\left[-\beta \left(E_i - E_{\min}\right)\right],
\qquad
p_i = \frac{w_i}{\sum_j w_j}
$$

This should be read as a reporting-friendly schematic, not as a claim that the current `patina`
implementation is frozen to this exact formula. The point is that lower-energy candidates receive
more weight while the method still retains a controllable amount of exploration.

## Runnable Rust: Toy Selection Pressure

```rust,editable,mdbook-runnable
fn main() {
    let energies = [-15.91_f64, -15.74, -15.19, -14.66];
    let beta = 3.0_f64;
    let e_min = energies.iter().copied().fold(f64::INFINITY, f64::min);

    let weights: Vec<f64> = energies
        .iter()
        .map(|energy| (-beta * (energy - e_min)).exp())
        .collect();
    let norm: f64 = weights.iter().sum();

    for (index, (energy, weight)) in energies.iter().zip(weights.iter()).enumerate() {
        let probability = weight / norm;
        println!(
            "candidate {index}: energy = {energy:>6.2}, selection_probability = {probability:.3}"
        );
    }
}
```

## What `patina` Should Keep Stable

Even if the codebase refactors, the public GA explanation should keep the following stable ideas:

- a run should expose generation metrics that summarize best, mean, and worst energy
- duplicate pressure should be visible rather than hidden inside silent operator failures
- repopulation and restart state should be preserved explicitly
- the external evaluator boundary should stay separate from core search semantics

## Build-Time Command Example

This section is generated during `mdbook build` from a tiny committed GA run directory. It proves
that the page is describing a real current tool surface rather than only a design sketch.

> [!NOTE]
> The command below runs `patina-tools energy-evolution` against a small tracked fixture and embeds
> the emitted JSON into the chapter during `mdbook build`.

<!-- cmdrun --strict bash ../../scripts/render-ga-energy-evolution-example.sh -->

## Reporting Interpretation

When the exact GA internals are still evolving, these are the signals worth emphasizing in public
documentation:

- whether best energy improved and by how much
- whether the population mean is also moving, not just the current champion
- whether duplicate pressure is rising
- whether repopulation or failure counts indicate search fragility
- which evaluator backend and workflow owner produced the run

Those signals matter more to the public narrative than embedding browser-only interactive panels.
The book should prefer stable explanatory prose and reproducible artifact references over decorative
widgets unless the widget adds scientific meaning that cannot be conveyed otherwise.
