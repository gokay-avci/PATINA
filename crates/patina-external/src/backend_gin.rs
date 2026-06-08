use patina_types::Candidate;
use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use crate::backend_adapters::{cartesian_coords, EvalError};

/// Marker separating immutable template content from the coordinate block injected by Rust.
pub const INJECTION_MARKER: &str = "# === KLMC3-RS COORDINATE INJECTION POINT ===";

/// Template-driven GIN writer.
///
/// Expected template format:
/// - all static header, keyword, species, and potential lines appear before
///   [`INJECTION_MARKER`]
/// - an optional validation line may appear before the marker as
///   `# KLMC3-RS EXPECT_ATOMS: <n>`
/// - this writer emits:
///   - `vectors` + three lattice lines + `0 0 0 0 0 0` when a lattice is present
///   - `fractional`
///   - one `species x y z` line per candidate site
///
/// This captures Phase 0's inferred file contract: the Rust layer only injects geometry while
/// leaving the scientifically coupled GULP keywords and potentials untouched in the template.
#[derive(Debug, Clone)]
pub struct GinWriter {
    template_path: PathBuf,
    template: GinTemplate,
}

#[derive(Debug, Clone)]
enum GinTemplate {
    Marker {
        template_prefix: String,
        expected_atoms: Option<usize>,
    },
    NativeCartesianCluster {
        header_lines: Vec<String>,
        coord_rows: Vec<NativeCoordRow>,
        footer_lines: Vec<String>,
    },
}

#[derive(Debug, Clone)]
struct NativeCoordRow {
    species: String,
    site_kind: String,
    suffix_tokens: Vec<String>,
}

impl GinWriter {
    /// Loads and validates a template from disk.
    pub fn from_template_file(path: impl AsRef<Path>) -> Result<Self, EvalError> {
        let template_path = path.as_ref().to_path_buf();
        let raw = fs::read_to_string(&template_path).map_err(EvalError::IoError)?;
        if let Some((prefix, _)) = raw.split_once(INJECTION_MARKER) {
            let expected_atoms = prefix
                .lines()
                .find_map(|line| line.trim().strip_prefix("# KLMC3-RS EXPECT_ATOMS:"))
                .map(|value| value.trim().parse::<usize>())
                .transpose()
                .map_err(|err| EvalError::TemplateInvalid {
                    path: template_path.clone(),
                    reason: format!("invalid atom-count declaration: {err}"),
                })?;

            return Ok(Self {
                template_path,
                template: GinTemplate::Marker {
                    template_prefix: prefix.trim_end().to_string(),
                    expected_atoms,
                },
            });
        }

        let native = parse_native_cartesian_cluster_template(&raw, &template_path)?;

        Ok(Self {
            template_path,
            template: native,
        })
    }

    /// Writes a complete `.gin` file for the candidate.
    pub fn write_candidate(
        &self,
        candidate: &Candidate,
        output_path: impl AsRef<Path>,
    ) -> Result<(), EvalError> {
        candidate
            .validate()
            .map_err(|err| EvalError::TemplateInvalid {
                path: self.template_path.clone(),
                reason: format!("candidate validation failed: {err:?}"),
            })?;

        let rendered = match &self.template {
            GinTemplate::Marker {
                template_prefix,
                expected_atoms,
            } => {
                if let Some(expected_atoms) = expected_atoms {
                    if candidate.len() != *expected_atoms {
                        return Err(EvalError::TemplateInvalid {
                            path: self.template_path.clone(),
                            reason: format!(
                                "candidate atom count {} does not match template expectation {}",
                                candidate.len(),
                                expected_atoms
                            ),
                        });
                    }
                }

                let mut rendered = String::new();
                rendered.push_str(template_prefix);
                rendered.push('\n');

                if let Some(lattice) = candidate.lattice {
                    rendered.push_str("vectors\n");
                    for vector in lattice {
                        rendered.push_str(&format!(
                            "{:>18.10} {:>18.10} {:>18.10}\n",
                            vector[0], vector[1], vector[2]
                        ));
                    }
                    rendered.push_str("0 0 0 0 0 0\n");
                }

                rendered.push_str("fractional\n");
                for (species, coord) in candidate.species.iter().zip(&candidate.fractional_coords) {
                    rendered.push_str(&format!(
                        "{species:<4} {:>14.9} {:>14.9} {:>14.9}\n",
                        coord[0], coord[1], coord[2]
                    ));
                }

                rendered
            }
            GinTemplate::NativeCartesianCluster {
                header_lines,
                coord_rows,
                footer_lines,
            } => render_native_cartesian_cluster(
                candidate,
                header_lines,
                coord_rows,
                footer_lines,
                &self.template_path,
            )?,
        };

        fs::write(output_path, rendered).map_err(EvalError::IoError)
    }

    /// Writes a restart XYZ aligned with the native template ordering when needed.
    pub fn write_restart_xyz(
        &self,
        candidate: &Candidate,
        output_path: impl AsRef<Path>,
    ) -> Result<(), EvalError> {
        candidate
            .validate()
            .map_err(|err| EvalError::TemplateInvalid {
                path: self.template_path.clone(),
                reason: format!("candidate validation failed: {err:?}"),
            })?;
        let ordered = match &self.template {
            GinTemplate::Marker { .. } => candidate
                .species
                .iter()
                .cloned()
                .zip(cartesian_coords(candidate))
                .collect::<Vec<_>>(),
            GinTemplate::NativeCartesianCluster { coord_rows, .. } => {
                candidate_rows_in_template_species_order(
                    candidate,
                    coord_rows,
                    &self.template_path,
                )?
            }
        };

        let mut rendered = String::new();
        rendered.push_str(&format!("{}\n", ordered.len()));
        rendered.push_str(&format!("label={}\n", candidate.label));
        for (species, coord) in ordered {
            rendered.push_str(&format!(
                "{species} {:.10} {:.10} {:.10}\n",
                coord[0], coord[1], coord[2]
            ));
        }
        fs::write(output_path, rendered).map_err(EvalError::IoError)
    }
}

fn parse_native_cartesian_cluster_template(
    raw: &str,
    template_path: &Path,
) -> Result<GinTemplate, EvalError> {
    let lines: Vec<String> = raw.lines().map(ToString::to_string).collect();
    let mut header_end = None;
    let mut footer_start = None;

    for (index, line) in lines.iter().enumerate() {
        let parts: Vec<_> = line.split_whitespace().collect();
        let site_kind = parts.get(1).map(|value| value.to_ascii_lowercase());
        let looks_like_native_coord_row = parts.len() >= 8
            && matches!(site_kind.as_deref(), Some("core") | Some("shel"))
            && parts[2].parse::<f64>().is_ok()
            && parts[3].parse::<f64>().is_ok()
            && parts[4].parse::<f64>().is_ok();
        if header_end.is_none() && looks_like_native_coord_row {
            header_end = Some(index);
        }
        let trimmed = line.trim().to_ascii_lowercase();
        if header_end.is_some() && trimmed == "species" {
            footer_start = Some(index);
            break;
        }
    }

    let (header_end, footer_start) = match (header_end, footer_start) {
        (Some(header_end), Some(footer_start)) if header_end < footer_start => {
            (header_end, footer_start)
        }
        _ => {
            return Err(EvalError::TemplateInvalid {
                path: template_path.to_path_buf(),
                reason: format!(
                    "missing injection marker `{INJECTION_MARKER}` and could not detect native cartesian cluster coordinate block"
                ),
            })
        }
    };

    let header_lines = lines[..header_end].to_vec();
    let footer_lines = lines[footer_start..].to_vec();
    let mut coord_rows = Vec::new();
    for (line_offset, line) in lines[header_end..footer_start].iter().enumerate() {
        if line.trim().is_empty() {
            continue;
        }
        let parts: Vec<_> = line.split_whitespace().collect();
        if parts.len() < 8 {
            return Err(EvalError::TemplateInvalid {
                path: template_path.to_path_buf(),
                reason: format!(
                    "native cartesian coordinate row {} is too short: `{}`",
                    header_end + line_offset + 1,
                    line
                ),
            });
        }
        coord_rows.push(NativeCoordRow {
            species: parts[0].to_string(),
            site_kind: parts[1].to_string(),
            suffix_tokens: parts[5..]
                .iter()
                .map(|value| (*value).to_string())
                .collect(),
        });
    }

    Ok(GinTemplate::NativeCartesianCluster {
        header_lines,
        coord_rows,
        footer_lines,
    })
}

fn render_native_cartesian_cluster(
    candidate: &Candidate,
    header_lines: &[String],
    coord_rows: &[NativeCoordRow],
    footer_lines: &[String],
    template_path: &Path,
) -> Result<String, EvalError> {
    let ordered = candidate_rows_in_template_species_order(candidate, coord_rows, template_path)?;
    let mut rendered = String::new();
    for line in header_lines {
        rendered.push_str(line);
        rendered.push('\n');
    }
    for ((species, coord), row) in ordered.iter().zip(coord_rows.iter()) {
        rendered.push_str(&format!(
            "{:<2} {:<4} {:>10.6} {:>10.6} {:>10.6}",
            species, row.site_kind, coord[0], coord[1], coord[2]
        ));
        for token in &row.suffix_tokens {
            rendered.push(' ');
            rendered.push_str(token);
        }
        rendered.push('\n');
    }
    for (index, line) in footer_lines.iter().enumerate() {
        rendered.push_str(line);
        if index + 1 < footer_lines.len() || raw_template_ended_with_newline(footer_lines) {
            rendered.push('\n');
        }
    }
    Ok(rendered)
}

fn raw_template_ended_with_newline(lines: &[String]) -> bool {
    !lines.is_empty()
}

fn candidate_rows_in_template_species_order(
    candidate: &Candidate,
    coord_rows: &[NativeCoordRow],
    template_path: &Path,
) -> Result<Vec<(String, [f64; 3])>, EvalError> {
    let coords = cartesian_coords(candidate);
    let mut coords_by_species: BTreeMap<String, Vec<[f64; 3]>> = BTreeMap::new();
    for (species, coord) in candidate.species.iter().zip(coords.iter()) {
        coords_by_species
            .entry(species.to_ascii_lowercase())
            .or_default()
            .push(*coord);
    }

    let mut row_counts_by_species: BTreeMap<String, usize> = BTreeMap::new();
    for row in coord_rows {
        *row_counts_by_species
            .entry(row.species.to_ascii_lowercase())
            .or_default() += 1;
    }

    let mut multiplicity_by_species: BTreeMap<String, usize> = BTreeMap::new();
    for (species, species_coords) in &coords_by_species {
        let Some(row_count) = row_counts_by_species.get(species).copied() else {
            return Err(EvalError::TemplateInvalid {
                path: template_path.to_path_buf(),
                reason: format!(
                    "native cluster template does not provide coordinate rows for candidate species `{}`",
                    species
                ),
            });
        };
        if species_coords.is_empty() {
            return Err(EvalError::TemplateInvalid {
                path: template_path.to_path_buf(),
                reason: format!(
                    "candidate has no coordinates for native cluster template species `{}`",
                    species
                ),
            });
        }
        if row_count % species_coords.len() != 0 {
            return Err(EvalError::TemplateInvalid {
                path: template_path.to_path_buf(),
                reason: format!(
                    "native cluster template row count {} for species `{}` is not compatible with candidate count {}",
                    row_count,
                    species,
                    species_coords.len()
                ),
            });
        }
        multiplicity_by_species.insert(species.clone(), row_count / species_coords.len());
    }

    let mut ordered = Vec::with_capacity(coord_rows.len());
    let mut row_usage_by_species: BTreeMap<String, usize> = BTreeMap::new();
    for row in coord_rows {
        let species_key = row.species.to_ascii_lowercase();
        let Some(species_coords) = coords_by_species.get(&species_key) else {
            return Err(EvalError::TemplateInvalid {
                path: template_path.to_path_buf(),
                reason: format!(
                    "candidate does not provide coordinates for native cluster template species `{}`",
                    row.species
                ),
            });
        };
        let multiplicity = multiplicity_by_species
            .get(&species_key)
            .copied()
            .unwrap_or(1)
            .max(1);
        let row_usage = row_usage_by_species.entry(species_key.clone()).or_default();
        let coord_index = *row_usage / multiplicity;
        if coord_index >= species_coords.len() {
            return Err(EvalError::TemplateInvalid {
                path: template_path.to_path_buf(),
                reason: format!(
                    "candidate ran out of coordinates for native cluster template species `{}`",
                    row.species
                ),
            });
        }
        ordered.push((row.species.clone(), species_coords[coord_index]));
        *row_usage += 1;
    }

    for (species, species_coords) in coords_by_species {
        let used_rows = row_usage_by_species.get(&species).copied().unwrap_or(0);
        let multiplicity = multiplicity_by_species
            .get(&species)
            .copied()
            .unwrap_or(1)
            .max(1);
        if used_rows != species_coords.len() * multiplicity {
            return Err(EvalError::TemplateInvalid {
                path: template_path.to_path_buf(),
                reason: format!(
                    "native cluster template consumed {} rows for species `{}` but expected {}",
                    used_rows,
                    species,
                    species_coords.len() * multiplicity
                ),
            });
        }
    }

    Ok(ordered)
}
