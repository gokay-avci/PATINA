set shell := ["bash", "-cu"]
set dotenv-load := true

root := justfile_directory()
uv_cache_dir := root + "/.uv-cache"
venvs_dir := root + "/venvs"
tools_env := venvs_dir + "/tools"
janus_env := venvs_dir + "/janus"
janus_python := janus_env + "/bin/python"
janus_adapter := root + "/crates/patina-external/python/janus_mace_adapter.py"
autoemulate_env := venvs_dir + "/autoemulate"
stk_env := venvs_dir + "/stk"
mpl_cache_dir := root + "/.mpl-cache"
nauty_dir := root + "/crates/patina-dreadnaut/nauty25r9"
bundled_dreadnaut := nauty_dir + "/dreadnaut"
nauty_provenance := nauty_dir + "/patina_build_provenance.txt"

default: help

help:
    @just --list

setup-venvs:
    mkdir -p "{{venvs_dir}}" "{{uv_cache_dir}}" "{{mpl_cache_dir}}"

setup-tools: setup-venvs
    env -u CONDA_PREFIX UV_CACHE_DIR="{{uv_cache_dir}}" UV_PROJECT_ENVIRONMENT="{{tools_env}}" uv sync --project "{{root}}" --native-tls

setup-janus: setup-venvs
    UV_CACHE_DIR="{{uv_cache_dir}}" uv venv "{{janus_env}}"
    UV_CACHE_DIR="{{uv_cache_dir}}" uv pip install --python "{{janus_python}}" "ase>=3.22" "janus-core[mace]"

setup-emulate: setup-venvs
    env -u CONDA_PREFIX UV_CACHE_DIR="{{uv_cache_dir}}" UV_PROJECT_ENVIRONMENT="{{autoemulate_env}}" uv sync --project "{{root}}/crates/patina-emulate/python" --extra autoemulate --native-tls

setup-stk: setup-venvs
    env -u CONDA_PREFIX UV_CACHE_DIR="{{uv_cache_dir}}" UV_PROJECT_ENVIRONMENT="{{stk_env}}" uv sync --project "{{root}}/crates/patina-stk/python" --native-tls

setup-all: setup-tools setup-janus setup-emulate setup-stk

setup-python: setup-tools setup-janus setup-emulate setup-stk

setup-macos-native-deps:
    @if [ "$(uname -s)" != "Darwin" ]; then printf '%s\n' 'setup-macos-native-deps is only needed on macOS'; exit 0; fi
    @if ! command -v brew >/dev/null 2>&1; then printf '%s\n' 'Homebrew is required for macOS native dependencies; install brew, then rerun `just setup-macos-native-deps`.' >&2; exit 1; fi
    @if [ -f /opt/homebrew/opt/libomp/lib/libomp.dylib ] || [ -f /usr/local/opt/libomp/lib/libomp.dylib ]; then printf '%s\n' 'libomp-ok'; else brew install libomp; fi

env-help:
    @printf '%s\n' \
      'Supported workspace environment variables:' \
      '  GULP_BIN=/abs/path/to/gulp' \
      '  PATINA_CRYSTAL_BIN=/abs/path/to/Pcrystal' \
      '  CRYSTAL_BIN=/abs/path/to/crystal/bin/directory-or-Pcrystal' \
      '  CRYSTAL_INSTALL_GUIDE=/abs/path/to/CRYSTAL23_how_to_install.pdf' \
      '  JANUS_PYTHON_BIN={{janus_python}}' \
      '  JANUS_ADAPTER={{janus_adapter}}' \
      '  PATINA_AUTOEMULATE_ENV={{autoemulate_env}}' \
      '  PATINA_STK_ENV={{stk_env}}' \
      '  PATINA_DREADNAUT_PATH={{bundled_dreadnaut}}' \
      '  MPLCONFIGDIR={{mpl_cache_dir}}' \
      '  PATINA_SITE_PROFILE=macbook-pro|dgx-spark|young|archer2' \
      '  CC=/abs/path/to/cc' \
      '' \
      'Local path setup:' \
      '  cp .env.example .env' \
      '  edit .env for this machine' \
      '  just env-export'

env-export:
    @printf '%s\n' \
      '# Source these exports in a shell, or keep them in .env for just recipes.' \
      "export GULP_BIN='${GULP_BIN:-}'" \
      "export PATINA_CRYSTAL_BIN='${PATINA_CRYSTAL_BIN:-}'" \
      "export CRYSTAL_BIN='${CRYSTAL_BIN:-}'" \
      "export CRYSTAL_INSTALL_GUIDE='${CRYSTAL_INSTALL_GUIDE:-}'" \
      "export JANUS_PYTHON_BIN='${JANUS_PYTHON_BIN:-{{janus_python}}}'" \
      "export JANUS_ADAPTER='${JANUS_ADAPTER:-{{janus_adapter}}}'" \
      "export PATINA_AUTOEMULATE_ENV='${PATINA_AUTOEMULATE_ENV:-{{autoemulate_env}}}'" \
      "export PATINA_STK_ENV='${PATINA_STK_ENV:-{{stk_env}}}'" \
      "export PATINA_DREADNAUT_PATH='${PATINA_DREADNAUT_PATH:-{{bundled_dreadnaut}}}'" \
      "export MPLCONFIGDIR='${MPLCONFIGDIR:-{{mpl_cache_dir}}}'" \
      "export PATINA_SITE_PROFILE='${PATINA_SITE_PROFILE:-}'"

doctor: doctor-python-envs doctor-external-paths
    @test -f "{{janus_adapter}}"
    @printf '%s\n' "janus adapter: {{janus_adapter}}"
    @if [ -x "{{janus_python}}" ]; then "{{janus_python}}" -c "import ase, janus_core; print('janus-ok')"; else printf '%s\n' "janus python not bootstrapped yet: {{janus_python}}"; fi

doctor-external-paths:
    @printf '%s\n' 'external path doctor:'
    @if [ -n "${GULP_BIN:-}" ]; then if [ -x "$GULP_BIN" ]; then printf '  gulp-ok: %s\n' "$GULP_BIN"; else printf '  gulp-not-executable: %s\n' "$GULP_BIN" >&2; exit 1; fi; else printf '%s\n' '  GULP_BIN is not set; set it in .env or export it before GULP-backed workflows.'; fi
    @crystal_bin="${PATINA_CRYSTAL_BIN:-${CRYSTAL_BIN:-}}"; if [ -d "$crystal_bin" ] && [ -x "$crystal_bin/Pcrystal" ]; then crystal_bin="$crystal_bin/Pcrystal"; fi; if [ -n "$crystal_bin" ]; then if [ -x "$crystal_bin" ] && [ ! -d "$crystal_bin" ]; then printf '  crystal-ok: %s\n' "$crystal_bin"; else printf '  crystal-not-executable: %s\n' "$crystal_bin" >&2; exit 1; fi; else printf '%s\n' '  PATINA_CRYSTAL_BIN is not set; set it in .env or export it before CRYSTAL-backed workflows.'; fi
    @if [ -n "${CRYSTAL_INSTALL_GUIDE:-}" ]; then if [ -f "$CRYSTAL_INSTALL_GUIDE" ]; then printf '  crystal-install-guide: %s\n' "$CRYSTAL_INSTALL_GUIDE"; else printf '  crystal-install-guide-missing: %s\n' "$CRYSTAL_INSTALL_GUIDE" >&2; exit 1; fi; fi

doctor-system:
    @printf '%s\n' 'system doctor:'
    @printf '  site-profile: %s\n' "${PATINA_SITE_PROFILE:-unset}"
    @printf '  uname: '; uname -a || true
    @printf '  machine: '; uname -m || true
    @printf '  shell: %s\n' "${SHELL:-unknown}"
    @if command -v rustc >/dev/null 2>&1; then printf '  rustc: '; rustc --version; else printf '%s\n' '  rustc: missing'; fi
    @if command -v cargo >/dev/null 2>&1; then printf '  cargo: '; cargo --version; else printf '%s\n' '  cargo: missing'; fi
    @cc_bin="${CC:-cc}"; printf '  cc: %s\n' "$cc_bin"; if command -v "$cc_bin" >/dev/null 2>&1; then "$cc_bin" --version 2>&1 | awk '/version|gcc|GCC|clang|Clang/ { print; found=1; exit } END { if (!found) print "version unavailable" }'; else printf '  cc-version: missing or not on PATH\n'; fi
    @if command -v python3 >/dev/null 2>&1; then printf '  python3: '; python3 --version; else printf '%s\n' '  python3: missing'; fi
    @if command -v uv >/dev/null 2>&1; then printf '  uv: '; uv --version; else printf '%s\n' '  uv: missing'; fi
    @if command -v just >/dev/null 2>&1; then printf '  just: '; just --version; else printf '%s\n' '  just: missing'; fi
    @if command -v file >/dev/null 2>&1; then printf '  file-tool: '; command -v file; else printf '%s\n' '  file-tool: missing'; fi
    @if [ "$(uname -s)" = "Darwin" ]; then if [ -f /opt/homebrew/opt/libomp/lib/libomp.dylib ] || [ -f /usr/local/opt/libomp/lib/libomp.dylib ]; then printf '%s\n' '  libomp: ok'; else printf '%s\n' '  libomp: missing; run `just setup-macos-native-deps` before AutoEmulate/LightGBM workflows'; fi; fi

doctor-python-envs:
    @printf '%s\n' 'python environment doctor:'
    @if [ -x "{{tools_env}}/bin/python" ]; then "{{tools_env}}/bin/python" -c "import pandas, plotly; print('tools-ok', pandas.__version__, plotly.__version__)"; else printf '%s\n' "tools env not bootstrapped yet: {{tools_env}}"; fi
    @if [ -x "{{stk_env}}/bin/python" ]; then "{{stk_env}}/bin/python" -c "import numpy, scipy, rdkit, patina_stk_runtime; print('stk-ok', numpy.__version__, scipy.__version__, rdkit.__version__)"; else printf '%s\n' "stk env not bootstrapped yet: {{stk_env}}"; fi
    @if [ -x "{{autoemulate_env}}/bin/python" ]; then MPLCONFIGDIR="{{mpl_cache_dir}}" "{{autoemulate_env}}/bin/python" -c "import ase, autoemulate, dscribe, featomic, gpytorch, numpy, pandas, sklearn, torch, patina_emulate_runtime; print('autoemulate-ok', numpy.__version__, pandas.__version__, torch.__version__)"; else printf '%s\n' "autoemulate env not bootstrapped yet: {{autoemulate_env}}"; fi

doctor-nauty:
    @dreadnaut="${PATINA_DREADNAUT_PATH:-{{bundled_dreadnaut}}}"; \
      printf '%s\n' 'nauty/dreadnaut doctor:'; \
      printf '  source-dir: %s\n' "{{nauty_dir}}"; \
      printf '  dreadnaut: %s\n' "$dreadnaut"; \
      if [ -f "{{nauty_provenance}}" ]; then \
        sed 's/^/  provenance: /' "{{nauty_provenance}}"; \
      else \
        printf '%s\n' '  provenance: missing; run `just build-nauty` on this machine'; \
      fi; \
      if command -v file >/dev/null 2>&1 && [ -e "$dreadnaut" ]; then \
        binary_info="$(file "$dreadnaut")"; \
        printf '  binary: %s\n' "$binary_info"; \
        machine="$(uname -m)"; \
        case "$machine" in \
          arm64|aarch64) case "$binary_info" in *arm64*|*aarch64*) ;; *) printf '  architecture-warning: host %s does not match binary description\n' "$machine";; esac ;; \
          x86_64|amd64) case "$binary_info" in *x86_64*|*x86-64*) ;; *) printf '  architecture-warning: host %s does not match binary description\n' "$machine";; esac ;; \
        esac; \
      fi
    @just --justfile "{{root}}/Justfile" smoke-nauty

build-nauty:
    @printf '%s\n' 'building bundled nauty/dreadnaut for this machine'
    cd "{{nauty_dir}}" && CC="${CC:-cc}" ./configure && make clean && make
    @dreadnaut="{{bundled_dreadnaut}}"; cc_bin="${CC:-cc}"; smoke_label="$(printf '%s\n' 'l=1000' 'c' 'n=2 g' '0 : 1 ;' '1 : 0 .' 'f=[0,|1,]' 'x' 'z' | "$dreadnaut" 2>&1 | awk '/^\[[^]]+\]$/ { value=$0 } END { print value }')"; { printf 'built_at=%s\n' "$(date -u '+%Y-%m-%dT%H:%M:%SZ')"; printf 'source_dir=%s\n' "{{nauty_dir}}"; printf 'dreadnaut=%s\n' "$dreadnaut"; printf 'uname=%s\n' "$(uname -a)"; printf 'machine=%s\n' "$(uname -m)"; printf 'cc=%s\n' "$cc_bin"; if command -v "$cc_bin" >/dev/null 2>&1; then printf 'cc_version=%s\n' "$("$cc_bin" --version 2>&1 | awk '/version|gcc|GCC|clang|Clang/ { print; found=1; exit } END { if (!found) print "version unavailable" }')"; else printf 'cc_version=missing\n'; fi; if command -v file >/dev/null 2>&1; then printf 'binary_file=%s\n' "$(file "$dreadnaut")"; fi; printf 'smoke_label=%s\n' "$smoke_label"; } > "{{nauty_provenance}}"
    @just --justfile "{{root}}/Justfile" smoke-nauty

smoke-nauty:
    @dreadnaut="${PATINA_DREADNAUT_PATH:-{{bundled_dreadnaut}}}"; if [ ! -x "$dreadnaut" ]; then printf 'dreadnaut is not executable: %s\n' "$dreadnaut" >&2; printf '%s\n' 'Set PATINA_DREADNAUT_PATH or run `just build-nauty` on this machine.' >&2; exit 1; fi; output="$(printf '%s\n' 'l=1000' 'c' 'n=2 g' '0 : 1 ;' '1 : 0 .' 'f=[0,|1,]' 'x' 'z' | "$dreadnaut" 2>&1)"; hashkey=""; while IFS= read -r line; do if [[ "$line" == \[*\] ]]; then hashkey="$line"; fi; done <<< "$output"; if [ -z "$hashkey" ]; then printf '%s\n' "$output" >&2; printf 'dreadnaut smoke failed: no canonical label from %s\n' "$dreadnaut" >&2; exit 1; fi; printf 'dreadnaut-smoke-ok: %s\n' "$hashkey"

doctor-ga-runtime: doctor-system doctor-nauty doctor-external-paths
    @test -f "{{janus_adapter}}"
    @printf '%s\n' "janus adapter: {{janus_adapter}}"
    @if [ -x "${JANUS_PYTHON_BIN:-{{janus_python}}}" ]; then "${JANUS_PYTHON_BIN:-{{janus_python}}}" -c "import ase, janus_core; print('janus-ok')"; else printf '%s\n' "janus python not bootstrapped yet: ${JANUS_PYTHON_BIN:-{{janus_python}}}"; fi

cargo-check:
    cargo check --workspace --all-targets

cargo-test:
    cargo test --workspace --all-targets

fmt:
    cargo fmt --all --check

fmt-fix:
    cargo fmt --all

verify-local: doctor-system doctor-nauty doctor-python-envs doctor-external-paths fmt cargo-check cargo-test

docs-bootstrap:
    cargo install --locked mdbook mdbook-mermaid mdbook-linkcheck mdbook-bib mdbook-tabs
    cargo build --manifest-path "{{root}}/tools/mdbook-cmdrun-compat/Cargo.toml"
    cargo build --manifest-path "{{root}}/tools/mdbook-bib-compat/Cargo.toml"
    cargo build --manifest-path "{{root}}/tools/mdbook-mermaid-compat/Cargo.toml"
    mdbook-mermaid install guide
    cd "{{root}}/guide" && mdbook-tabs install

docs-build:
    mdbook build guide

docs-serve:
    mdbook serve guide

docs-test:
    mdbook test guide

docs-clean:
    mdbook clean guide
