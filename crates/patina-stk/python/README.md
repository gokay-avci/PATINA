# patina-stk Python Runtime

This directory is the Python adapter runtime for `patina-stk`.

Intended ownership:

- RDKit-backed canonicalization and sanitization
- SMARTS-based functional-group detection
- optional conformer embedding for fragment preparation

Environment policy:

- install into the workspace-managed environment `venvs/stk`
- do not create a crate-local virtual environment
- keep the adapter contract narrow and file/JSON based
- prefer `just setup-stk` from the repo root

Runtime policy:

- invoke the runtime with `venvs/stk/bin/python`
- keep `PYTHONPATH` pointed at `crates/patina-stk/python/src` during development
- use `uv sync --project crates/patina-stk/python` with `UV_PROJECT_ENVIRONMENT` set to `venvs/stk`

Bootstrap example:

```bash
ROOT="$(pwd)"
mkdir -p "$ROOT/venvs"
UV_CACHE_DIR="$ROOT/.uv-cache" \
UV_PROJECT_ENVIRONMENT="$ROOT/venvs/stk" \
uv sync --project "$ROOT/crates/patina-stk/python" --native-tls
```

Smoke test:

```bash
ROOT="$(pwd)"
PYTHONPATH="$ROOT/crates/patina-stk/python/src" \
"$ROOT/venvs/stk/bin/python" -m patina_stk_runtime status
```

