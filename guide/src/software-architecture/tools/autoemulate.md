# 6.6.3 Autoemulate Surrogate Runtime

<p class="lead">
  `patina-emulate` is the surrogate-learning lane for `patina`: Rust owns orchestration and feature
  seams, while the Python project under `crates/patina-emulate/python` supplies the ML-heavy runtime
  inside the workspace-managed `venvs/autoemulate` environment.
</p>

## At A Glance

- Scientific role:
  Advisory search acceleration. This lane exists to rank, guide, or prioritize expensive evaluation
  work.
- Current ownership:
  Rust owns runtime contracts and feature-projector seams, while the model stack lives in a
  dedicated Python project instead of leaking across the workspace.
- Status:
  Research lane, not parity lane. The surrogate path should stay clearly labeled as advisory until
  there is preserved evidence for stronger claims.

## Stable Idea

The stable value here is not one specific model family. It is the workflow boundary:

- expensive scientific evaluation remains explicit
- surrogate models can advise search without silently redefining it
- Python-heavy ML dependencies stay outside the Rust scientific core

That boundary will still make sense even if the modeling stack changes later.

For the scientific tutorial track, see
[4.2.2 AutoEmulate Materials Tutorial Seed](../../scientific-foundations/emulators/autoemulate-materials-tutorial.md).

{{#tabs global="lens"}}
{{#tab name="Materials Lens"}}

From the chemistry side, this lane is useful when the search space is too large to evaluate
uniformly and some learned prioritization is scientifically worthwhile.

{{#endtab}}
{{#tab name="Software Lens"}}

From the software side, the main point is runtime hygiene:

- use the workspace-managed `venvs/autoemulate`
- keep the Python project in one obvious place
- expose typed Rust-side seams for feature projection and orchestration

{{#endtab}}
{{#tab name="Bridge"}}

The bridge rule is caution. Surrogate guidance may help decide where to spend computation, but the
provenance record should still say when a result came from a learned model rather than an evaluator.

{{#endtab}}
{{#endtabs}}

## Boundary

Owned today:

- Rust-side feature-projector seams
- Python project ownership under `crates/patina-emulate/python`
- environment naming and resolution through `venvs/autoemulate`
- downstream candidate scoring through `score-ga-emulate`
- request and response artifacts that preserve the surrogate handoff
- research-only uncertainty-gated staged evaluation

Deferred or intentionally constrained:

- any claim that emulate-guided search is native SCOTT parity
- silent mutation of deterministic parity-lane semantics
- a polished public property tutorial with a successful, preserved response artifact
