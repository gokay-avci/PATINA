# 4.1.3 Monte Carlo Search

<p class="lead">
  Monte Carlo search is the temperature-mediated exploration family that connects plain Metropolis
  style search, simulated annealing, and threshold-oriented variants already present in the
  <code>patina</code> workflow vocabulary.
</p>

## Core Acceptance Rule

The basic Metropolis-style idea is straightforward:

$$
P_{\mathrm{acc}} =
\begin{cases}
1, & \Delta E \le 0 \\
\exp\left(-\frac{\Delta E}{k_B T}\right), & \Delta E > 0
\end{cases}
$$

At high temperature, uphill moves are tolerated more often. As temperature is reduced, the method
behaves more greedily.

## Why It Matters Here

In `patina`, Monte Carlo style search is not only an algorithmic category. It is also a reporting
category that helps explain:

- simulated annealing schedules
- energy-lid or threshold workflows
- restart-sensitive search trajectories
- the difference between raw proposal generation and accepted state history

## Literature Position

Within the broader structure-prediction literature surveyed by @woodley2008csp, Monte Carlo and
annealing-style methods remain important because they expose the exploration-versus-exploitation
tradeoff directly through temperature or threshold policy. For `patina`, that makes them useful not
only as search methods but also as reporting methods: a trajectory becomes interpretable when the
acceptance schedule is explicit rather than buried in implementation detail.

## Runnable Rust: Temperature Effect

```rust,editable,mdbook-runnable
fn acceptance(delta_e: f64, temperature: f64) -> f64 {
    if delta_e <= 0.0 {
        1.0
    } else {
        (-delta_e / temperature).exp()
    }
}

fn main() {
    let delta_e = 0.08_f64;
    for temperature in [1.0_f64, 0.4, 0.1] {
        println!(
            "T = {temperature:.2}, acceptance = {:.3}",
            acceptance(delta_e, temperature)
        );
    }
}
```

## Stable Public Reading

The exact search schedule can change as the workspace evolves. The public method description should
therefore stay focused on stable questions:

- how temperature or threshold policy shapes acceptance
- how accepted trajectories are recorded
- how restart state and provenance are preserved
- how these workflows differ from population-based GA and relaxation-based basin hopping

For `patina`, this page is the place to explain the method family. Exact campaign presets and
workflow details can stay in software-architecture pages and provenance notes.
