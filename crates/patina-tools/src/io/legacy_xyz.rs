use patina_types::Candidate;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::Path;
use thiserror::Error;

#[derive(Debug, Error, PartialEq)]
pub enum LegacyXyzError {
    #[error("xyz file `{path}` does not exist")]
    MissingFile { path: String },
    #[error("xyz file `{path}` is empty")]
    EmptyFile { path: String },
    #[error("xyz file `{path}` has invalid atom count `{value}`")]
    InvalidAtomCount { path: String, value: String },
    #[error("xyz file `{path}` is missing the comment line")]
    MissingCommentLine { path: String },
    #[error("xyz file `{path}` has invalid atom line {line_number}: `{line}`")]
    InvalidAtomLine {
        path: String,
        line_number: usize,
        line: String,
    },
    #[error("xyz file `{path}` declared {declared} atoms but contained {actual}")]
    AtomCountMismatch {
        path: String,
        declared: usize,
        actual: usize,
    },
    #[error("xyz file `{path}` has invalid numeric value `{value}` at line {line_number}")]
    InvalidNumericValue {
        path: String,
        line_number: usize,
        value: String,
    },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LegacyXyzStructure {
    pub label: String,
    pub candidate: Candidate,
    pub total_energy: Option<f64>,
    pub charges: Vec<f64>,
    pub cell_dims: Option<[f64; 3]>,
    pub comment: String,
}

impl LegacyXyzStructure {
    pub fn atom_count(&self) -> usize {
        self.candidate.len()
    }
}

pub fn read_legacy_xyz(path: &Path) -> Result<LegacyXyzStructure, LegacyXyzError> {
    if !path.is_file() {
        return Err(LegacyXyzError::MissingFile {
            path: path.display().to_string(),
        });
    }

    let raw = fs::read_to_string(path).map_err(|_| LegacyXyzError::MissingFile {
        path: path.display().to_string(),
    })?;
    parse_legacy_xyz_str(&raw, path)
}

pub fn parse_legacy_xyz_str(
    raw: &str,
    source_path: &Path,
) -> Result<LegacyXyzStructure, LegacyXyzError> {
    let path_label = source_path.display().to_string();
    let mut lines = raw.lines();
    let atom_count_line = lines.next().ok_or_else(|| LegacyXyzError::EmptyFile {
        path: path_label.clone(),
    })?;
    let atom_count =
        atom_count_line
            .trim()
            .parse::<usize>()
            .map_err(|_| LegacyXyzError::InvalidAtomCount {
                path: path_label.clone(),
                value: atom_count_line.trim().to_string(),
            })?;

    let comment = lines
        .next()
        .ok_or_else(|| LegacyXyzError::MissingCommentLine {
            path: path_label.clone(),
        })?
        .trim()
        .to_string();
    let metadata = parse_comment_metadata(&comment);

    let mut species = Vec::with_capacity(atom_count);
    let mut coords = Vec::with_capacity(atom_count);
    let mut charges = Vec::with_capacity(atom_count);

    for (offset, line) in lines.take(atom_count).enumerate() {
        let line_number = offset + 3;
        let parts = line.split_whitespace().collect::<Vec<_>>();
        if parts.len() < 4 {
            return Err(LegacyXyzError::InvalidAtomLine {
                path: path_label.clone(),
                line_number,
                line: line.to_string(),
            });
        }

        species.push(parts[0].to_string());
        coords.push([
            parse_f64(parts[1], &path_label, line_number)?,
            parse_f64(parts[2], &path_label, line_number)?,
            parse_f64(parts[3], &path_label, line_number)?,
        ]);
        charges.push(if parts.len() > 4 {
            parse_f64(parts[4], &path_label, line_number)?
        } else {
            0.0
        });
    }

    if species.len() != atom_count {
        return Err(LegacyXyzError::AtomCountMismatch {
            path: path_label,
            declared: atom_count,
            actual: species.len(),
        });
    }

    let label = source_path
        .file_stem()
        .and_then(|stem| stem.to_str())
        .unwrap_or("xyz")
        .to_string();

    Ok(LegacyXyzStructure {
        label: label.clone(),
        candidate: Candidate::cluster(label, species, coords),
        total_energy: metadata.total_energy,
        charges,
        cell_dims: metadata.cell_dims,
        comment,
    })
}

pub fn write_legacy_xyz(
    structure: &LegacyXyzStructure,
    output_path: &Path,
) -> Result<(), std::io::Error> {
    let mut rendered = String::new();
    rendered.push_str(&format!("{}\n", structure.atom_count()));
    if let Some(cell_dims) = structure.cell_dims {
        rendered.push_str(&format!(
            "{:.10} {:.10} {:.10}\n",
            cell_dims[0], cell_dims[1], cell_dims[2]
        ));
    } else if let Some(energy) = structure.total_energy.filter(|value| *value != 0.0) {
        rendered.push_str(&format!("SCF Done             {:.10e};\n", energy));
    } else {
        rendered.push('\n');
    }

    for (index, (species, coord)) in structure
        .candidate
        .species
        .iter()
        .zip(structure.candidate.fractional_coords.iter())
        .enumerate()
    {
        if let Some(charge) = structure.charges.get(index).copied() {
            rendered.push_str(&format!(
                "{species} {:.10} {:.10} {:.10} {:.2}\n",
                coord[0], coord[1], coord[2], charge
            ));
        } else {
            rendered.push_str(&format!(
                "{species} {:.10} {:.10} {:.10}\n",
                coord[0], coord[1], coord[2]
            ));
        }
    }

    fs::write(output_path, rendered)
}

#[derive(Debug, Clone, Copy, PartialEq)]
struct ParsedCommentMetadata {
    total_energy: Option<f64>,
    cell_dims: Option<[f64; 3]>,
}

fn parse_comment_metadata(comment: &str) -> ParsedCommentMetadata {
    let tokens = comment.split_whitespace().collect::<Vec<_>>();
    if comment.contains("SCF Done") && tokens.len() >= 3 {
        if let Ok(value) = tokens[2].trim_end_matches(';').parse::<f64>() {
            return ParsedCommentMetadata {
                total_energy: Some(value),
                cell_dims: None,
            };
        }
    }
    if comment.contains("Generated by KLMC DM") && tokens.len() >= 5 {
        if let Ok(value) = tokens[4].parse::<f64>() {
            return ParsedCommentMetadata {
                total_energy: Some(value),
                cell_dims: None,
            };
        }
    }
    if comment.contains("Generated by KLMC") && tokens.len() >= 4 {
        if let Ok(value) = tokens[3].parse::<f64>() {
            return ParsedCommentMetadata {
                total_energy: Some(value),
                cell_dims: None,
            };
        }
    }
    if let Some(value) = tokens
        .iter()
        .find_map(|token| parse_energy_assignment_token(token))
    {
        return ParsedCommentMetadata {
            total_energy: Some(value),
            cell_dims: None,
        };
    }
    if let Some(value) = parse_energy_assignment_token(comment) {
        return ParsedCommentMetadata {
            total_energy: Some(value),
            cell_dims: None,
        };
    }
    if tokens.len() == 3 {
        let parsed = [
            tokens[0].parse::<f64>().ok(),
            tokens[1].parse::<f64>().ok(),
            tokens[2].parse::<f64>().ok(),
        ];
        if let [Some(x), Some(y), Some(z)] = parsed {
            return ParsedCommentMetadata {
                total_energy: None,
                cell_dims: Some([x, y, z]),
            };
        }
    }

    ParsedCommentMetadata {
        total_energy: None,
        cell_dims: None,
    }
}

fn parse_energy_assignment_token(token: &str) -> Option<f64> {
    token
        .strip_prefix("energy=")
        .and_then(|value| value.trim_end_matches([';', ',']).parse::<f64>().ok())
}

fn parse_f64(value: &str, path: &str, line_number: usize) -> Result<f64, LegacyXyzError> {
    value
        .parse::<f64>()
        .map_err(|_| LegacyXyzError::InvalidNumericValue {
            path: path.to_string(),
            line_number,
            value: value.to_string(),
        })
}

#[cfg(test)]
mod tests {
    use super::{parse_legacy_xyz_str, write_legacy_xyz, LegacyXyzStructure};
    use patina_types::Candidate;
    use std::path::Path;
    use tempfile::tempdir;

    #[test]
    fn parses_energy_from_scf_done_comment() {
        let xyz =
            "2\nSCF Done             -1.2340000000e+01;\nMg 0.0 0.0 0.0 0.0\nO 1.0 0.0 0.0 0.0\n";
        let parsed = parse_legacy_xyz_str(xyz, Path::new("cluster.xyz")).expect("parse xyz");
        assert_eq!(parsed.total_energy, Some(-12.34));
        assert_eq!(parsed.atom_count(), 2);
    }

    #[test]
    fn parses_cell_dims_from_three_number_comment() {
        let xyz = "1\n10.0 11.0 12.0\nMg 0.0 0.0 0.0\n";
        let parsed = parse_legacy_xyz_str(xyz, Path::new("periodic.xyz")).expect("parse xyz");
        assert_eq!(parsed.cell_dims, Some([10.0, 11.0, 12.0]));
        assert_eq!(parsed.total_energy, None);
    }

    #[test]
    fn parses_energy_from_bare_energy_assignment_comment() {
        let xyz = "1\nenergy=-4.6258880898e+02\nMg 0.0 0.0 0.0\n";
        let parsed = parse_legacy_xyz_str(xyz, Path::new("cluster.xyz")).expect("parse xyz");
        assert_eq!(parsed.total_energy, Some(-462.58880898));
        assert_eq!(parsed.cell_dims, None);
    }

    #[test]
    fn round_trips_basic_xyz_rendering() {
        let xyz =
            "2\nSCF Done             -1.0000000000e+00;\nMg 0.0 0.0 0.0 0.1\nO 1.0 0.0 0.0 -0.1\n";
        let parsed = parse_legacy_xyz_str(xyz, Path::new("cluster.xyz")).expect("parse xyz");

        let dir = tempdir().expect("tempdir");
        let output = dir.path().join("roundtrip.xyz");
        write_legacy_xyz(&parsed, &output).expect("write xyz");
        let reparsed = parse_legacy_xyz_str(
            &std::fs::read_to_string(&output).expect("read output"),
            &output,
        )
        .expect("parse output");

        assert_eq!(reparsed.total_energy, parsed.total_energy);
        assert_eq!(reparsed.candidate.species, parsed.candidate.species);
        assert_eq!(reparsed.charges, parsed.charges);
    }

    #[test]
    fn omits_charge_column_when_structure_has_no_charges() {
        let structure = LegacyXyzStructure {
            label: "cluster".into(),
            candidate: Candidate::cluster(
                "cluster",
                vec!["Mg".into(), "O".into()],
                vec![[0.0, 0.0, 0.0], [1.0, 0.0, 0.0]],
            ),
            total_energy: None,
            charges: Vec::new(),
            cell_dims: None,
            comment: String::new(),
        };

        let dir = tempdir().expect("tempdir");
        let output = dir.path().join("no_charges.xyz");
        write_legacy_xyz(&structure, &output).expect("write xyz");
        let written = std::fs::read_to_string(&output).expect("read output");
        let atom_lines = written.lines().skip(2).collect::<Vec<_>>();

        assert_eq!(atom_lines.len(), 2);
        assert_eq!(atom_lines[0].split_whitespace().count(), 4);
        assert_eq!(atom_lines[1].split_whitespace().count(), 4);
    }
}
