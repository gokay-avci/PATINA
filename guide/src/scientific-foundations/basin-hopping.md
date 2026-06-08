# 4.1.2 Basin Hopping

<p class="lead">
  Basin hopping is the cleanest example of a method that is simultaneously a scientific search
  strategy and a software orchestration problem in <code>patina</code>.
</p>

## Core Idea

Basin hopping alternates between perturbation and relaxation. Instead of reasoning on the raw
landscape alone, it effectively reasons on the basin-minimized landscape introduced by Wales and
Doye @wales1997.

$$
\widetilde{E}(x) = \min_{\xi \in \mathcal{R}(x)} E(\xi)
$$

where \(\mathcal{R}(x)\) denotes the local relaxation started from perturbation state \(x\).

An acceptance rule then compares relaxed states rather than only raw proposed coordinates:

$$
P_{\mathrm{acc}} = \min \left(1, \exp\left[-\frac{\widetilde{E}(x') - \widetilde{E}(x)}{k_B T}\right]\right)
$$

## Why It Matters In `patina`

The durable basin-hopping story for this project is:

- proposal and move-regime logic should stay explicit
- relaxation and external evaluation belong behind a typed boundary
- restart continuity matters because basin hopping is often run in long campaigns
- matched-topology logic can change how a step should be interpreted, not just how it is reported later

The current Rust ownership is already substantial enough to say this method has a real home in the
workspace, even though exact parity details can still move.

## Literature Position

The basin-hopping baseline remains the Wales and Doye transformed-landscape formulation
@wales1997. In the wider materials-structure-prediction context surveyed by @woodley2008csp, that
matters because basin hopping keeps the conceptual split between:

- move generation
- local relaxation
- acceptance policy

That split maps well onto `patina`, where the proposal logic, evaluator boundary, and provenance
surface should remain explicit even if the exact code layout evolves.

## Runnable Rust: Acceptance Sketch

```rust,editable,mdbook-runnable
fn metropolis_accept(delta_e: f64, temperature: f64) -> f64 {
    if delta_e <= 0.0 {
        1.0
    } else {
        (-delta_e / temperature).exp()
    }
}

fn main() {
    let current = -18.42_f64;
    let proposed = -18.31_f64;
    let delta = proposed - current;
    let probability = metropolis_accept(delta, 0.12);
    println!("delta_e = {delta:.3}");
    println!("acceptance_probability = {probability:.3}");
}
```

## Durable Documentation Rule

This method page should keep explaining the relaxation-plus-acceptance logic even if the exact
crate or module names change. The public reader cares about:

- what a hop is
- what is relaxed
- what is accepted or rejected
- which artifacts make the trajectory traceable

For the architectural explanation of why this maps well onto ports and adapters, see
[6.1 Hexagonal Ports And Adapters](../software-architecture/hexagonal-architecture.md).
