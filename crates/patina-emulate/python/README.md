# PATINA Emulate Python Runtime

This is the dedicated Python runtime for `patina-emulate`.

It exists for one reason:

- keep AutoEmulate and custom SVGP models isolated from the Janus Python lane

That separation is intentional. The Rust side should pass normalized records into this runtime and
receive scored candidates back. This project must not import backend-specific Janus or GULP logic.

## Environment Policy

- install into the workspace-managed environment `venvs/autoemulate`
- do not create a crate-local virtual environment
- prefer `just setup-emulate` from the repo root

Manual bootstrap fallback:

```bash
ROOT="$(pwd)"
mkdir -p "$ROOT/venvs"
env -u CONDA_PREFIX \
  UV_CACHE_DIR="$ROOT/.uv-cache" \
  UV_PROJECT_ENVIRONMENT="$ROOT/venvs/autoemulate" \
  uv sync --project "$ROOT/crates/patina-emulate/python" --extra autoemulate --native-tls
```

Smoke test:

```bash
ROOT="$(pwd)"
PYTHONPATH="$ROOT/crates/patina-emulate/python/src" \
"$ROOT/venvs/autoemulate/bin/python" -m patina_emulate_runtime.cli --help
```

## Hexagonal Boundary

Upstream into this runtime:

- branch metadata from `patina-emulate`
- observed candidate feature vectors and targets
- pending candidate feature vectors
- surrogate policy selection

Downstream back into Rust:

- predicted means
- predicted variances
- acquisition scores
- ranked candidate order
- model provenance
- optional SVGP checkpoint path

The runtime only sees normalized campaign records. It does not know how Janus, GULP, or Scott
produced the observations.
