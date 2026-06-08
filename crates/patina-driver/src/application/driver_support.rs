use anyhow::{anyhow, Context, Result};
use patina_evaluator::ScottBackendMode;
use patina_external::{JanusMode, JanusOptimizer};
use patina_search::AtomSpec;
use patina_types::Candidate;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::fs;
use std::hash::{Hash, Hasher};
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EvalBackendKind {
    Scott,
    Gulp,
    JanusMace,
}

impl EvalBackendKind {
    pub fn engine_label(self) -> &'static str {
        match self {
            Self::Scott => "SCOTT+GULP",
            Self::Gulp => "GULP",
            Self::JanusMace => "Janus/MACE",
        }
    }

    pub fn provenance_id(self) -> &'static str {
        match self {
            Self::Scott => "scott",
            Self::Gulp => "gulp",
            Self::JanusMace => "janus_mace",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum JanusModeSetting {
    SinglePoint,
    LocalOpt,
}

impl JanusModeSetting {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::SinglePoint => "single-point",
            Self::LocalOpt => "local-opt",
        }
    }
}

impl From<JanusModeSetting> for JanusMode {
    fn from(value: JanusModeSetting) -> Self {
        match value {
            JanusModeSetting::SinglePoint => JanusMode::SinglePoint,
            JanusModeSetting::LocalOpt => JanusMode::LocalOpt,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum JanusOptimizerSetting {
    Lbfgs,
    Fire,
    Fire2,
    AbcFire,
}

impl JanusOptimizerSetting {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Lbfgs => "lbfgs",
            Self::Fire => "fire",
            Self::Fire2 => "fire2",
            Self::AbcFire => "abc-fire",
        }
    }
}

impl From<JanusOptimizerSetting> for JanusOptimizer {
    fn from(value: JanusOptimizerSetting) -> Self {
        match value {
            JanusOptimizerSetting::Lbfgs => JanusOptimizer::Lbfgs,
            JanusOptimizerSetting::Fire => JanusOptimizer::Fire,
            JanusOptimizerSetting::Fire2 => JanusOptimizer::Fire2,
            JanusOptimizerSetting::AbcFire => JanusOptimizer::AbcFire,
        }
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ScottBackendSettings {
    pub evaluator_backend: Option<String>,
    pub atoms_in_template: Option<PathBuf>,
    pub janus_python: Option<PathBuf>,
    pub janus_adapter: Option<PathBuf>,
    pub janus_mode: Option<String>,
    pub janus_arch: Option<String>,
    pub janus_model: Option<String>,
    pub janus_device: Option<String>,
    pub janus_dtype: Option<String>,
    pub janus_fmax: Option<f64>,
    pub janus_steps: Option<usize>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ScottRuntimeRoutingSettings {
    pub default_backend: Option<String>,
    #[serde(default)]
    pub stage_overrides: BTreeMap<u8, String>,
}

pub fn candidate_from_xyz(path: &Path) -> Result<Candidate> {
    super::scott_topology_export::parse_candidate_snapshot(path)
}

pub fn read_candidate_json(path: &Path) -> Result<Candidate> {
    let candidate_raw = fs::read_to_string(path)
        .with_context(|| format!("failed to read candidate JSON from `{}`", path.display()))?;
    let candidate: Candidate = serde_json::from_str(&candidate_raw)
        .with_context(|| format!("failed to parse candidate JSON from `{}`", path.display()))?;
    candidate.validate().map_err(|err| {
        anyhow!(
            "candidate from `{}` failed validation: {err:?}",
            path.display()
        )
    })?;
    Ok(candidate)
}

pub fn required_arg<T: Clone>(value: Option<T>, flag: &str, backend: &str) -> Result<T> {
    value.ok_or_else(|| anyhow!("`--{flag}` is required when backend is `{backend}`"))
}

pub fn absolutize_path(path: &Path) -> Result<PathBuf> {
    if path.is_absolute() {
        return Ok(path.to_path_buf());
    }
    let cwd = std::env::current_dir().context("failed to resolve current working directory")?;
    Ok(cwd.join(path))
}

pub fn parse_runtime_stage_overrides(values: &[String]) -> Result<BTreeMap<u8, String>> {
    let mut overrides = BTreeMap::new();
    for value in values {
        let Some((stage_raw, backend_raw)) = value.split_once('=') else {
            return Err(anyhow!(
                "invalid `--runtime-stage-backend` value `{value}`; expected `<stage>=<backend>`"
            ));
        };
        let stage = stage_raw.trim().parse::<u8>().with_context(|| {
            format!("invalid stage index `{stage_raw}` in `--runtime-stage-backend`")
        })?;
        if stage == 0 {
            return Err(anyhow!(
                "invalid stage index `0` in `--runtime-stage-backend`; Scott stages are one-based"
            ));
        }
        overrides.insert(stage, backend_raw.trim().to_string());
    }
    Ok(overrides)
}

pub fn parse_scott_backend_mode_label(raw: &str) -> Result<ScottBackendMode> {
    match raw.trim().to_ascii_lowercase().as_str() {
        "gulp" => Ok(ScottBackendMode::Gulp),
        "janus" | "janus_mace" | "janus-mace" | "mace" => Ok(ScottBackendMode::JanusMace),
        other => Err(anyhow!(
            "unsupported Scott runtime backend mode `{other}`; expected `gulp` or `janus_mace`"
        )),
    }
}

pub fn candidate_dimensionality_label(candidate: &Candidate) -> &'static str {
    match candidate.declared_dimensionality() {
        patina_types::StructureDimensionality::ZeroD => "0D",
        patina_types::StructureDimensionality::OneD => "1D",
        patina_types::StructureDimensionality::TwoD => "2D",
        patina_types::StructureDimensionality::ThreeD => "3D",
    }
}

pub fn candidate_dimensionality_support_label(candidate: &Candidate) -> &'static str {
    if candidate.supports_native_scott_search() {
        "native-parity-search-eligible"
    } else if candidate.has_partial_periodicity() {
        "topology-and-data-only"
    } else {
        "representable"
    }
}

pub fn ensure_native_scott_search_candidate(candidate: &Candidate, context: &str) -> Result<()> {
    if candidate.supports_native_scott_search() {
        return Ok(());
    }

    Err(anyhow!(
        "{context} does not support {} candidates in the native-parity lane; support_status={}. Current Rust support for this dimensionality is limited to representation, parsing, topology, and shared science rather than GA/BH/staged-native workflow execution.",
        candidate_dimensionality_label(candidate),
        candidate_dimensionality_support_label(candidate),
    ))
}

pub fn candidate_hashkey_cache_key(candidate: &Candidate) -> u64 {
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    candidate.species.len().hash(&mut hasher);
    for species in &candidate.species {
        species.hash(&mut hasher);
    }
    for coords in &candidate.fractional_coords {
        coords[0].to_bits().hash(&mut hasher);
        coords[1].to_bits().hash(&mut hasher);
        coords[2].to_bits().hash(&mut hasher);
    }
    if let Some(lattice) = &candidate.lattice {
        for row in lattice {
            row[0].to_bits().hash(&mut hasher);
            row[1].to_bits().hash(&mut hasher);
            row[2].to_bits().hash(&mut hasher);
        }
    }
    hasher.finish()
}

pub fn build_external_hashkey(
    candidate: &Candidate,
    atom_specs: Option<&[super::scott_topology_types::AtomSpecRecord]>,
    radius_mode: &str,
    radius_const: f64,
    hkg_path: &Path,
    scratch_dir: &Path,
    suffix: &str,
) -> Result<Option<String>> {
    build_external_hashkey_with_radius_offset(
        candidate,
        atom_specs,
        radius_mode,
        radius_const,
        0.0,
        hkg_path,
        scratch_dir,
        suffix,
    )
}

#[allow(clippy::too_many_arguments)]
pub fn build_external_hashkey_with_radius_offset(
    candidate: &Candidate,
    atom_specs: Option<&[super::scott_topology_types::AtomSpecRecord]>,
    radius_mode: &str,
    radius_const: f64,
    radius_offset: f64,
    hkg_path: &Path,
    scratch_dir: &Path,
    suffix: &str,
) -> Result<Option<String>> {
    let atom_specs = atom_specs.map(|records| {
        records
            .iter()
            .map(|record| AtomSpec {
                species: record.species.clone(),
                covalent_radius: record.covalent_radius,
                ionic_radius: record.ionic_radius,
            })
            .collect::<Vec<_>>()
    });
    let atom_specs =
        patina_dreadnaut::infer_atom_specs_for_candidate(candidate, atom_specs.as_deref())?;
    let radius = patina_dreadnaut::compute_hashkey_radius(
        candidate,
        &atom_specs,
        radius_mode,
        radius_const,
    )? + radius_offset;
    let graph_text = patina_dreadnaut::build_dreadnaut_graph_text(candidate, radius, &atom_specs);
    let graph_path = scratch_dir.join(format!("duplicate_parity_{suffix}.dreadnaut"));
    fs::write(&graph_path, graph_text)
        .with_context(|| format!("failed to write `{}`", graph_path.display()))?;
    let dreadnaut_path = resolve_hashkey_dreadnaut_path(hkg_path)?;
    let hash = patina_dreadnaut::canonical_hashkey_from_graph_file(&dreadnaut_path, &graph_path)
        .with_context(|| {
            format!(
                "failed to generate Dreadnaut hashkey via `{}` for `{}`",
                dreadnaut_path.display(),
                candidate.label
            )
        })?;
    if hash.is_empty() {
        return Err(anyhow!(
            "Dreadnaut returned an empty hashkey via `{}` for `{}`",
            dreadnaut_path.display(),
            candidate.label
        ));
    }
    Ok(Some(hash))
}

pub fn resolve_hashkey_dreadnaut_path(configured_path: &Path) -> Result<PathBuf> {
    let bundled_path = patina_dreadnaut::bundled_dreadnaut_path();
    let looks_like_legacy_wrapper = configured_path
        .extension()
        .and_then(|value| value.to_str())
        .is_some_and(|value| value.eq_ignore_ascii_case("py"))
        || configured_path
            .file_name()
            .and_then(|value| value.to_str())
            .is_some_and(|value| value.contains("hkg_dreadnaut_wrapper"));
    if looks_like_legacy_wrapper || configured_path == bundled_path {
        return patina_dreadnaut::resolve_dreadnaut_path(None);
    }
    patina_dreadnaut::resolve_dreadnaut_path(Some(configured_path))
}

pub fn verify_hashkey_dreadnaut_adapter(configured_path: &Path, context: &str) -> Result<PathBuf> {
    let dreadnaut_path = resolve_hashkey_dreadnaut_path(configured_path)
        .with_context(|| format!("{context}: failed to resolve Dreadnaut adapter"))?;
    let hash = patina_dreadnaut::canonical_hashkey_from_graph_text(
        &dreadnaut_path,
        dreadnaut_smoke_graph_text(),
    )
    .with_context(|| {
        format!(
            "{context}: Dreadnaut adapter smoke test failed for `{}`; run `just doctor-nauty` or rebuild with `just build-nauty`",
            dreadnaut_path.display()
        )
    })?;
    if hash.is_empty() {
        return Err(anyhow!(
            "{context}: Dreadnaut adapter `{}` returned an empty smoke-test hashkey",
            dreadnaut_path.display()
        ));
    }
    Ok(dreadnaut_path)
}

fn dreadnaut_smoke_graph_text() -> &'static str {
    "l=1000\nc\nn=2 g\n0 : 1 ;\n1 : 0 .\nf=[0,|1,]\nx\nz\n"
}

pub fn latest_surrogate_artifact_dir(root: &Path) -> Result<Option<PathBuf>> {
    let mut dirs = fs::read_dir(root)
        .with_context(|| format!("failed to read surrogate output root `{}`", root.display()))?
        .filter_map(|entry| entry.ok().map(|entry| entry.path()))
        .filter(|path| path.is_dir() && path.join("request.json").is_file())
        .collect::<Vec<_>>();
    dirs.sort_by(|left, right| {
        let left_name = left
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("");
        let right_name = right
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("");
        left_name.cmp(right_name)
    });
    Ok(dirs.pop())
}

#[cfg(test)]
mod tests {
    use super::{resolve_hashkey_dreadnaut_path, verify_hashkey_dreadnaut_adapter};
    use std::fs;

    #[test]
    fn explicit_missing_dreadnaut_path_fails_resolution() {
        let missing = tempfile::tempdir()
            .expect("tempdir")
            .path()
            .join("missing-dreadnaut");
        let error = resolve_hashkey_dreadnaut_path(&missing).expect_err("missing path must fail");
        assert!(
            error.to_string().contains("does not exist"),
            "unexpected error: {error:?}"
        );
    }

    #[test]
    fn explicit_non_dreadnaut_executable_fails_smoke_preflight() {
        let tempdir = tempfile::tempdir().expect("tempdir");
        let fake = tempdir.path().join("fake-dreadnaut");
        fs::write(&fake, "#!/bin/sh\nprintf 'not a canonical label\\n'\n").expect("write fake");
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mut permissions = fs::metadata(&fake).expect("metadata").permissions();
            permissions.set_mode(0o755);
            fs::set_permissions(&fake, permissions).expect("chmod fake");
        }

        let error =
            verify_hashkey_dreadnaut_adapter(&fake, "test preflight").expect_err("smoke fails");
        let message = format!("{error:#}");
        assert!(
            message.contains("smoke test failed") || message.contains("no canonical hashkey"),
            "unexpected error: {message}"
        );
    }
}
