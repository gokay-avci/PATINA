use anyhow::{anyhow, bail, Context, Result};
use patina_dreadnaut::{
    build_dreadnaut_graph_text, bundled_dreadnaut_path, canonical_hashkey_from_graph_file,
    compute_hashkey_radius, infer_atom_specs_for_candidate, resolve_dreadnaut_path,
};
use patina_perturber::ClusterStructure;
use patina_sci_kernel::codec::xyz::candidate_from_xyz_path;
use patina_types::Candidate;
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExpectedHashkey {
    pub fixture_name: String,
    pub hashkey: String,
}

pub fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .expect("repo root should resolve")
}

pub fn default_dreadnaut_path() -> PathBuf {
    bundled_dreadnaut_path()
}

pub fn as_ga_fixture_dir() -> PathBuf {
    repo_root()
        .join("extra")
        .join("to_integrate_project")
        .join("clusters ")
        .join("As-Ga")
}

pub fn as_ga_expected_manifest_path() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("fixtures")
        .join("native_hashkeys")
        .join("as_ga_expected_hashkeys.txt")
}

pub fn load_atom_specs(path: &Path) -> Result<Vec<patina_search::AtomSpec>> {
    patina_dreadnaut::parse_atoms_file(path)
}

pub fn load_xyz_candidate(path: &Path) -> Result<Candidate> {
    candidate_from_xyz_path(path)
        .map_err(|err| anyhow!("failed to decode xyz fixture `{}`: {err:?}", path.display()))
}

pub fn load_xyz_cluster_structure(path: &Path) -> Result<ClusterStructure> {
    let candidate = load_xyz_candidate(path)?;
    ClusterStructure::try_from_candidate(&candidate).map_err(|err| {
        anyhow!(
            "failed to convert `{}` into cluster structure: {err}",
            path.display()
        )
    })
}

pub fn infer_builtin_atom_specs(candidate: &Candidate) -> Result<Vec<patina_search::AtomSpec>> {
    infer_atom_specs_for_candidate(candidate, None)
}

pub fn load_expected_hashkeys(path: &Path) -> Result<Vec<ExpectedHashkey>> {
    let text = fs::read_to_string(path).with_context(|| {
        format!(
            "failed to read expected hashkey manifest `{}`",
            path.display()
        )
    })?;
    let mut rows = Vec::new();
    for (line_index, line) in text.lines().enumerate() {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }
        let mut parts = trimmed.split_whitespace();
        let fixture_name = parts
            .next()
            .ok_or_else(|| anyhow!("manifest line {} is missing a fixture name", line_index + 1))?;
        let hashkey = parts
            .next()
            .ok_or_else(|| anyhow!("manifest line {} is missing a hashkey", line_index + 1))?;
        if parts.next().is_some() {
            bail!(
                "manifest line {} must contain exactly two fields: `<fixture.xyz> <hashkey>`",
                line_index + 1
            );
        }
        rows.push(ExpectedHashkey {
            fixture_name: fixture_name.to_string(),
            hashkey: hashkey.to_string(),
        });
    }
    Ok(rows)
}

pub fn discover_xyz_fixtures(dir: &Path) -> Result<Vec<PathBuf>> {
    let mut fixtures = fs::read_dir(dir)
        .with_context(|| format!("failed to read fixture dir `{}`", dir.display()))?
        .filter_map(|entry| entry.ok().map(|entry| entry.path()))
        .filter(|path| {
            path.extension()
                .and_then(|ext| ext.to_str())
                .map(|ext| ext.eq_ignore_ascii_case("xyz"))
                .unwrap_or(false)
        })
        .collect::<Vec<_>>();
    fixtures.sort();
    Ok(fixtures)
}

pub fn compute_native_compatible_hashkey(
    candidate: &Candidate,
    atom_specs: &[patina_search::AtomSpec],
    dreadnaut_path: &Path,
    scratch_dir: &Path,
    fixture_label: &str,
) -> Result<String> {
    fs::create_dir_all(scratch_dir).with_context(|| {
        format!(
            "failed to create hashkey scratch dir `{}`",
            scratch_dir.display()
        )
    })?;

    let radius = compute_hashkey_radius(candidate, atom_specs, "IR", 0.0)?;
    let graph_text = build_dreadnaut_graph_text(candidate, radius, atom_specs);
    let graph_path = scratch_dir.join(format!("{fixture_label}.dreadnaut"));
    fs::write(&graph_path, graph_text)
        .with_context(|| format!("failed to write `{}`", graph_path.display()))?;
    let dreadnaut_path = resolve_dreadnaut_path(Some(dreadnaut_path))?;
    canonical_hashkey_from_graph_file(&dreadnaut_path, &graph_path)
}

#[cfg(test)]
mod tests {
    use super::{
        as_ga_fixture_dir, build_dreadnaut_graph_text, discover_xyz_fixtures,
        infer_builtin_atom_specs, load_xyz_candidate,
    };

    #[test]
    #[ignore = "requires legacy As-Ga fixture layout outside the generic workspace baseline"]
    fn as_ga_fixtures_exist() {
        let fixtures = discover_xyz_fixtures(&as_ga_fixture_dir()).expect("discover fixtures");
        assert!(!fixtures.is_empty());
    }

    #[test]
    fn builtin_atom_specs_contain_as_and_ga() {
        let candidate = patina_types::Candidate {
            species: vec!["As".into(), "Ga".into()],
            fractional_coords: vec![[0.0, 0.0, 0.0], [1.0, 0.0, 0.0]],
            lattice: None,
            periodic_axes: [false, false, false],
            label: "as_ga".into(),
        };
        let atom_specs = infer_builtin_atom_specs(&candidate).expect("infer atoms");
        assert!(atom_specs.iter().any(|record| record.species == "As"));
        assert!(atom_specs.iter().any(|record| record.species == "Ga"));
    }

    #[test]
    #[ignore = "requires legacy As-Ga fixture layout outside the generic workspace baseline"]
    fn dreadnaut_graph_contains_partition_block_for_as_ga_fixture() {
        let fixture = discover_xyz_fixtures(&as_ga_fixture_dir())
            .expect("discover fixtures")
            .into_iter()
            .next()
            .expect("fixture");
        let candidate = load_xyz_candidate(&fixture).expect("load candidate");
        let atom_specs = infer_builtin_atom_specs(&candidate).expect("infer atoms");
        let graph = build_dreadnaut_graph_text(&candidate, 3.0, &atom_specs);
        assert!(graph.contains("\nf=["));
        assert!(graph.ends_with("\nz"));
    }
}
