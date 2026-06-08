# 7.1 Legacy Runtime Model

The older `KLMC3` ecosystem supplied several concepts that still matter during migration:

- staged external-program execution
- template-driven GULP or SCOTT input generation
- atom-spec tables such as `atoms.in`
- run-directory artifacts that later Rust tools inspect or replay

The public `patina` story should treat these as legacy contracts to describe, not mandatory local
directory assumptions to preserve.

> [!WARNING]
> Any path spelled like a specific workstation directory or a repo sibling such as `KLMC3-main/...`
> is migration scaffolding, not acceptable public setup guidance.

The migration task is therefore:

- keep the scientific meaning
- keep the auditable provenance
- remove hardcoded filesystem expectations
