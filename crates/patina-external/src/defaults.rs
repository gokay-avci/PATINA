use std::path::PathBuf;

/// Default Python interpreter used by Janus-backed workflows when the caller does not override it.
pub const DEFAULT_JANUS_PYTHON_BIN: &str = "venvs/janus/bin/python";

/// Default adapter script path kept under the owning crate instead of the repo-level `scripts/`.
pub const DEFAULT_JANUS_ADAPTER_SCRIPT: &str =
    "crates/patina-external/python/janus_mace_adapter.py";

pub fn default_janus_python_bin() -> PathBuf {
    PathBuf::from(DEFAULT_JANUS_PYTHON_BIN)
}

pub fn default_janus_adapter_script() -> PathBuf {
    PathBuf::from(DEFAULT_JANUS_ADAPTER_SCRIPT)
}
