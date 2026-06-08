# patina-emulate

`patina-emulate` is the surrogate-learning crate for PATINA workflows.

It has two parts:

- Rust contracts, runtime orchestration, and feature-projection seams in this crate
- a Python project under `crates/patina-emulate/python`

## Python Ownership

Unlike most crates in this workspace, `patina-emulate` does own a Python project.

The intended pairing is:

- project: `crates/patina-emulate/python`
- environment: `venvs/autoemulate`

Preferred bootstrap command:

```text
just setup-emulate
```

## Runtime Policy

When a bare environment name such as `autoemulate` is used, the runtime resolves it under the
workspace-managed `venvs/` directory rather than under a crate-local `.venv`.

## See Also

- [python/README.md](python/README.md)
