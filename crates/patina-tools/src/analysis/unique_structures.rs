use crate::io::legacy_xyz::{read_legacy_xyz, write_legacy_xyz, LegacyXyzStructure};
use patina_dreadnaut::{
    build_dreadnaut_graph_text, canonical_hashkey_from_graph_text, compute_hashkey_radius,
    infer_atom_specs_for_candidate, parse_atoms_file, resolve_dreadnaut_path, AtomSpec,
};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum UniqueStructuresError {
    #[error("input root `{0}` does not exist")]
    MissingInputRoot(PathBuf),
    #[error("failed to parse atoms override file `{path}`: {source}")]
    AtomsFile {
        path: PathBuf,
        #[source]
        source: anyhow::Error,
    },
    #[error("failed to resolve dreadnaut path: {0}")]
    DreadnautResolution(#[source] anyhow::Error),
    #[error("failed to read legacy xyz `{path}`: {source}")]
    ReadXyz {
        path: PathBuf,
        #[source]
        source: crate::io::legacy_xyz::LegacyXyzError,
    },
    #[error("failed to compute hashkey for `{path}`: {source}")]
    Hashkey {
        path: PathBuf,
        #[source]
        source: anyhow::Error,
    },
    #[error("failed to create output directory `{0}`")]
    CreateOutputDirectory(PathBuf),
    #[error("failed to write unique structure `{path}`: {source}")]
    WriteOutput {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error("failed to write summary `{path}`: {source}")]
    WriteSummary {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct UniqueStructuresConfig {
    pub input_root: PathBuf,
    pub extension: String,
    pub atoms_path: Option<PathBuf>,
    pub dreadnaut_path: Option<PathBuf>,
    pub hashkey_radius_mode: String,
    pub hashkey_radius_const: f64,
}

impl UniqueStructuresConfig {
    pub fn xyz(input_root: impl Into<PathBuf>) -> Self {
        Self {
            input_root: input_root.into(),
            extension: "xyz".into(),
            atoms_path: None,
            dreadnaut_path: None,
            hashkey_radius_mode: "IR".into(),
            hashkey_radius_const: 3.34,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct UniqueStructureEntry {
    pub rank: usize,
    pub label: String,
    pub energy: Option<f64>,
    pub hashkey: String,
    pub duplicate_count: usize,
    pub source_path: PathBuf,
    pub output_file_name: String,
    pub structure: LegacyXyzStructure,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct UniqueStructuresReport {
    pub scanned_file_count: usize,
    pub unique_count: usize,
    pub entries: Vec<UniqueStructureEntry>,
}

pub fn run_unique_structures_workflow(
    config: &UniqueStructuresConfig,
) -> Result<UniqueStructuresReport, UniqueStructuresError> {
    let files = discover_files_recursive(&config.input_root, &config.extension)?;
    let atom_specs_override = config
        .atoms_path
        .as_deref()
        .map(parse_atoms_file)
        .transpose()
        .map_err(|source| UniqueStructuresError::AtomsFile {
            path: config
                .atoms_path
                .clone()
                .unwrap_or_else(|| PathBuf::from("<none>")),
            source,
        })?;
    let dreadnaut_path = resolve_dreadnaut_path(config.dreadnaut_path.as_deref())
        .map_err(UniqueStructuresError::DreadnautResolution)?;

    let mut unique_by_hashkey = BTreeMap::<String, UniqueAccumulator>::new();

    for path in &files {
        let structure = read_legacy_xyz(path).map_err(|source| UniqueStructuresError::ReadXyz {
            path: path.clone(),
            source,
        })?;
        let atom_specs =
            infer_atom_specs_for_candidate(&structure.candidate, atom_specs_override.as_deref())
                .map_err(|source| UniqueStructuresError::Hashkey {
                    path: path.clone(),
                    source,
                })?;
        let hashkey = compute_legacy_hashkey(
            &structure,
            &atom_specs,
            &dreadnaut_path,
            &config.hashkey_radius_mode,
            config.hashkey_radius_const,
        )
        .map_err(|source| UniqueStructuresError::Hashkey {
            path: path.clone(),
            source,
        })?;

        unique_by_hashkey
            .entry(hashkey.clone())
            .and_modify(|entry| entry.observe(path, &structure))
            .or_insert_with(|| UniqueAccumulator::new(path.clone(), structure, hashkey));
    }

    let mut entries = unique_by_hashkey
        .into_values()
        .map(UniqueAccumulator::into_entry)
        .collect::<Vec<_>>();

    entries.sort_by(|left, right| {
        compare_optional_energy(left.energy, right.energy)
            .then_with(|| left.label.cmp(&right.label))
            .then_with(|| left.source_path.cmp(&right.source_path))
    });

    for (index, entry) in entries.iter_mut().enumerate() {
        entry.rank = index + 1;
        entry.output_file_name = format!(
            "n{:02}_{:03}_{}.xyz",
            entry.structure.atom_count(),
            entry.rank,
            entry.label
        );
    }

    Ok(UniqueStructuresReport {
        scanned_file_count: files.len(),
        unique_count: entries.len(),
        entries,
    })
}

pub fn write_unique_structures_outputs(
    report: &UniqueStructuresReport,
    output_dir: &Path,
) -> Result<(), UniqueStructuresError> {
    fs::create_dir_all(output_dir)
        .map_err(|_| UniqueStructuresError::CreateOutputDirectory(output_dir.to_path_buf()))?;

    for entry in &report.entries {
        let output_path = output_dir.join(&entry.output_file_name);
        write_legacy_xyz(&entry.structure, &output_path).map_err(|source| {
            UniqueStructuresError::WriteOutput {
                path: output_path,
                source,
            }
        })?;
    }

    let summary_path = output_dir.join("unique_structures.csv");
    let mut summary =
        String::from("rank,label,energy,hashkey,duplicate_count,source_path,output_file\n");
    for entry in &report.entries {
        let energy = entry
            .energy
            .map(|value| value.to_string())
            .unwrap_or_default();
        summary.push_str(&format!(
            "{},{},{},{},{},{},{}\n",
            entry.rank,
            entry.label,
            energy,
            entry.hashkey,
            entry.duplicate_count,
            entry.source_path.display(),
            entry.output_file_name
        ));
    }
    fs::write(&summary_path, summary).map_err(|source| UniqueStructuresError::WriteSummary {
        path: summary_path,
        source,
    })?;

    Ok(())
}

fn discover_files_recursive(
    input_root: &Path,
    extension: &str,
) -> Result<Vec<PathBuf>, UniqueStructuresError> {
    if !input_root.exists() {
        return Err(UniqueStructuresError::MissingInputRoot(
            input_root.to_path_buf(),
        ));
    }

    let mut files = Vec::new();
    collect_matching_files(input_root, extension, &mut files)?;
    files.sort();
    Ok(files)
}

fn collect_matching_files(
    current: &Path,
    extension: &str,
    files: &mut Vec<PathBuf>,
) -> Result<(), UniqueStructuresError> {
    for entry in fs::read_dir(current)
        .map_err(|_| UniqueStructuresError::MissingInputRoot(current.to_path_buf()))?
    {
        let entry =
            entry.map_err(|_| UniqueStructuresError::MissingInputRoot(current.to_path_buf()))?;
        let path = entry.path();
        if path.is_dir() {
            collect_matching_files(&path, extension, files)?;
            continue;
        }
        let matches = path
            .extension()
            .and_then(|ext| ext.to_str())
            .map(|ext| ext.eq_ignore_ascii_case(extension))
            .unwrap_or(false);
        if matches {
            files.push(path);
        }
    }

    Ok(())
}

fn compute_legacy_hashkey(
    structure: &LegacyXyzStructure,
    atom_specs: &[AtomSpec],
    dreadnaut_path: &Path,
    radius_mode: &str,
    radius_const: f64,
) -> Result<String, anyhow::Error> {
    let radius =
        compute_hashkey_radius(&structure.candidate, atom_specs, radius_mode, radius_const)?;
    let graph = build_dreadnaut_graph_text(&structure.candidate, radius, atom_specs);
    canonical_hashkey_from_graph_text(dreadnaut_path, &graph)
}

#[derive(Debug, Clone)]
struct UniqueAccumulator {
    source_path: PathBuf,
    structure: LegacyXyzStructure,
    hashkey: String,
    duplicate_count: usize,
}

impl UniqueAccumulator {
    fn new(source_path: PathBuf, structure: LegacyXyzStructure, hashkey: String) -> Self {
        Self {
            source_path,
            structure,
            hashkey,
            duplicate_count: 0,
        }
    }

    fn observe(&mut self, path: &Path, structure: &LegacyXyzStructure) {
        self.duplicate_count += 1;
        if compare_optional_energy(structure.total_energy, self.structure.total_energy).is_lt() {
            self.source_path = path.to_path_buf();
            self.structure = structure.clone();
        }
    }

    fn into_entry(self) -> UniqueStructureEntry {
        UniqueStructureEntry {
            rank: 0,
            label: self.structure.label.clone(),
            energy: self.structure.total_energy,
            hashkey: self.hashkey,
            duplicate_count: self.duplicate_count,
            source_path: self.source_path,
            output_file_name: String::new(),
            structure: self.structure,
        }
    }
}

fn compare_optional_energy(left: Option<f64>, right: Option<f64>) -> std::cmp::Ordering {
    match (left, right) {
        (Some(left), Some(right)) => left.total_cmp(&right),
        (Some(_), None) => std::cmp::Ordering::Less,
        (None, Some(_)) => std::cmp::Ordering::Greater,
        (None, None) => std::cmp::Ordering::Equal,
    }
}

#[cfg(test)]
mod tests {
    use super::{
        run_unique_structures_workflow, write_unique_structures_outputs, UniqueStructuresConfig,
    };
    use std::fs;
    use tempfile::tempdir;

    #[cfg(unix)]
    fn write_fake_dreadnaut(dir: &std::path::Path) -> std::path::PathBuf {
        let path = dir.join("fake_dreadnaut.sh");
        std::fs::write(
            &path,
            "#!/bin/sh\ninput=$(cat)\ncase \"$input\" in\n  *\"n=3 g\"*) printf '[3]\\n' ;;\n  *) printf '[2]\\n' ;;\nesac\n",
        )
        .expect("write fake dreadnaut");
        let mut permissions = std::fs::metadata(&path)
            .expect("fake dreadnaut metadata")
            .permissions();
        use std::os::unix::fs::PermissionsExt;
        permissions.set_mode(0o755);
        std::fs::set_permissions(&path, permissions).expect("chmod fake dreadnaut");
        path
    }

    #[cfg(unix)]
    #[test]
    fn deduplicates_structures_by_hashkey_and_keeps_lowest_energy() {
        let dir = tempdir().expect("tempdir");
        let input = dir.path().join("inputs");
        fs::create_dir_all(&input).expect("inputs dir");
        fs::write(
            input.join("a.xyz"),
            "2\nSCF Done             -1.0000000000e+01;\nMg 0.0 0.0 0.0\nO 1.0 0.0 0.0\n",
        )
        .expect("write a");
        fs::write(
            input.join("b.xyz"),
            "2\nSCF Done             -1.1000000000e+01;\nMg 0.0 0.0 0.0\nO 1.0 0.0 0.0\n",
        )
        .expect("write b");
        fs::write(
            input.join("c.xyz"),
            "3\nSCF Done             -9.0000000000e+00;\nMg 0.0 0.0 0.0\nO 0.0 1.0 0.0\nO 0.0 0.0 1.0\n",
        )
        .expect("write c");

        let mut config = UniqueStructuresConfig::xyz(&input);
        config.dreadnaut_path = Some(write_fake_dreadnaut(dir.path()));
        let report = run_unique_structures_workflow(&config).expect("run unique workflow");

        assert_eq!(report.scanned_file_count, 3);
        assert_eq!(report.unique_count, 2);
        assert_eq!(report.entries[0].energy, Some(-11.0));
        assert_eq!(report.entries[0].duplicate_count, 1);
    }

    #[cfg(unix)]
    #[test]
    fn writes_unique_outputs_and_summary() {
        let dir = tempdir().expect("tempdir");
        let input = dir.path().join("inputs");
        let output = dir.path().join("unique");
        fs::create_dir_all(&input).expect("inputs dir");
        fs::write(
            input.join("single.xyz"),
            "2\nSCF Done             -1.0000000000e+01;\nMg 0.0 0.0 0.0\nO 1.0 0.0 0.0\n",
        )
        .expect("write single");

        let mut config = UniqueStructuresConfig::xyz(&input);
        config.dreadnaut_path = Some(write_fake_dreadnaut(dir.path()));
        let report = run_unique_structures_workflow(&config).expect("run unique workflow");
        write_unique_structures_outputs(&report, &output).expect("write outputs");

        let summary = fs::read_to_string(output.join("unique_structures.csv")).expect("summary");
        assert!(summary.contains("rank,label,energy,hashkey"));
        assert!(fs::read_dir(&output)
            .expect("read output")
            .any(|entry| entry
                .ok()
                .and_then(|entry| entry.file_name().into_string().ok())
                .map(|name| name.ends_with(".xyz"))
                .unwrap_or(false)));
    }
}
