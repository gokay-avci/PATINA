# 6. Software Architecture

<p class="lead">
  This lane is for readers who want to understand how `patina` is engineered so that scientific
  workflows stay inspectable, reproducible across machines, and ready for public release.
</p>

## Entry Points

- [Architecture](../patina/architecture.html):
  Read this first if the main question is how domain logic, application services, and external
  adapters are separated.
- [Development Environment](../patina/development-environment.html):
  This path explains how Python environments, external runtimes, and machine-local setup stay
  explicit rather than hidden.
- [Tooling Landscape](tools/):
  This cluster records the stable meaning of ARVO, Dreadnaut, `stk`, SYVA, RASPA-style work,
  surrogate modeling, and the perturber stack.
- [Runtime And Campaign Boundaries](runtime-boundaries.html):
  Read this when the main question is how driver services, external evaluators, schedulers, and
  active-learning advice stay separated.

The software-architecture lane is not only for developers. It is also where scientific readers can inspect the
controls that protect provenance:

- which crates own which responsibilities
- how local setup remains reproducible across machines
- how external tools are kept at explicit boundaries
- how architecture supports scientific integrity instead of competing with it

If you are reading the codebase before reading the science, this is your main lane.
