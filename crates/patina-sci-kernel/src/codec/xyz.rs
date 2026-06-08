use serde::{Deserialize, Serialize};
use std::fmt::Write as _;
use std::fs;
use std::path::Path;
use thiserror::Error;

#[cfg(feature = "bridge-patina-types")]
use patina_types::Candidate;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AtomRecord {
    pub species: String,
    pub coords: [f64; 3],
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct XyzFrame {
    pub atom_count: usize,
    pub comment: String,
    pub atoms: Vec<AtomRecord>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum XyzCoordinateMode {
    Stored,
    CartesianFromLattice,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct XyzEncodeOptions {
    pub comment: Option<String>,
    pub coordinate_mode: XyzCoordinateMode,
    pub extxyz_fields: bool,
    pub label: Option<String>,
    pub energy: Option<f64>,
}

impl Default for XyzEncodeOptions {
    fn default() -> Self {
        Self {
            comment: None,
            coordinate_mode: XyzCoordinateMode::Stored,
            extxyz_fields: false,
            label: None,
            energy: None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum XyzError {
    #[error("empty xyz input")]
    EmptyInput,
    #[error("xyz frame is missing the comment line")]
    MissingCommentLine,
    #[error("invalid xyz atom count line `{line}`")]
    InvalidAtomCount { line: String },
    #[error("unexpected EOF while reading xyz frame: expected {expected_atoms} atoms, found {found_atoms}")]
    UnexpectedEof {
        expected_atoms: usize,
        found_atoms: usize,
    },
    #[error("invalid xyz atom line {line_number}: `{line}`")]
    InvalidAtomLine { line_number: usize, line: String },
    #[error("invalid coordinate `{value}` on axis {axis} at line {line_number}")]
    InvalidCoordinate {
        line_number: usize,
        axis: usize,
        value: String,
    },
    #[error("{message}")]
    Io { message: String },
    #[cfg(feature = "bridge-patina-types")]
    #[error("invalid candidate for xyz bridge: {message}")]
    InvalidCandidate { message: String },
}

pub fn encode_xyz_frames(frames: &[XyzFrame]) -> Result<String, XyzError> {
    if frames.is_empty() {
        return Err(XyzError::EmptyInput);
    }
    let mut rendered = String::new();
    for frame in frames {
        writeln!(&mut rendered, "{}", frame.atom_count).expect("string write");
        writeln!(&mut rendered, "{}", frame.comment).expect("string write");
        for atom in &frame.atoms {
            writeln!(
                &mut rendered,
                "{} {:.16e} {:.16e} {:.16e}",
                atom.species, atom.coords[0], atom.coords[1], atom.coords[2]
            )
            .expect("string write");
        }
    }
    Ok(rendered)
}

pub fn write_xyz_frames(path: &Path, frames: &[XyzFrame]) -> Result<(), XyzError> {
    let rendered = encode_xyz_frames(frames)?;
    fs::write(path, rendered).map_err(|err| XyzError::Io {
        message: format!("failed to write `{}`: {err}", path.display()),
    })
}

pub fn parse_xyz_frames(text: &str) -> Result<Vec<XyzFrame>, XyzError> {
    let lines = text.lines().collect::<Vec<_>>();
    if lines.is_empty() {
        return Err(XyzError::EmptyInput);
    }
    let mut cursor = 0usize;
    let mut frames = Vec::new();
    while cursor < lines.len() {
        while cursor < lines.len() && lines[cursor].trim().is_empty() {
            cursor += 1;
        }
        if cursor >= lines.len() {
            break;
        }
        let count_line = lines[cursor].trim().to_string();
        let atom_count = count_line
            .parse::<usize>()
            .map_err(|_| XyzError::InvalidAtomCount {
                line: count_line.clone(),
            })?;
        cursor += 1;
        if cursor >= lines.len() {
            return Err(XyzError::MissingCommentLine);
        }
        let comment = lines[cursor].to_string();
        cursor += 1;

        let mut atoms = Vec::with_capacity(atom_count);
        for atom_index in 0..atom_count {
            if cursor >= lines.len() {
                return Err(XyzError::UnexpectedEof {
                    expected_atoms: atom_count,
                    found_atoms: atom_index,
                });
            }
            let line_number = cursor + 1;
            let line = lines[cursor];
            cursor += 1;
            let parts = line.split_whitespace().collect::<Vec<_>>();
            if parts.len() < 4 {
                return Err(XyzError::InvalidAtomLine {
                    line_number,
                    line: line.to_string(),
                });
            }
            let mut coords = [0.0; 3];
            for axis in 0..3 {
                coords[axis] =
                    parts[axis + 1]
                        .parse::<f64>()
                        .map_err(|_| XyzError::InvalidCoordinate {
                            line_number,
                            axis,
                            value: parts[axis + 1].to_string(),
                        })?;
            }
            atoms.push(AtomRecord {
                species: parts[0].to_string(),
                coords,
            });
        }

        frames.push(XyzFrame {
            atom_count,
            comment,
            atoms,
        });
    }
    Ok(frames)
}

pub fn read_xyz_frames(path: &Path) -> Result<Vec<XyzFrame>, XyzError> {
    let text = fs::read_to_string(path).map_err(|err| XyzError::Io {
        message: format!("failed to read `{}`: {err}", path.display()),
    })?;
    parse_xyz_frames(&text)
}

pub fn parse_extxyz_lattice(comment: &str) -> Result<Option<[[f64; 3]; 3]>, XyzError> {
    let Some(start) = comment.find("Lattice=\"") else {
        return Ok(None);
    };
    let rest = &comment[start + "Lattice=\"".len()..];
    let Some(end) = rest.find('"') else {
        return Ok(None);
    };
    let values = rest[..end]
        .replace(',', " ")
        .split_whitespace()
        .map(str::parse::<f64>)
        .collect::<Result<Vec<_>, _>>()
        .map_err(|_| XyzError::InvalidCoordinate {
            line_number: 2,
            axis: 0,
            value: rest[..end].to_string(),
        })?;
    if values.len() != 9 {
        return Ok(None);
    }
    Ok(Some([
        [values[0], values[1], values[2]],
        [values[3], values[4], values[5]],
        [values[6], values[7], values[8]],
    ]))
}

pub fn parse_extxyz_pbc(comment: &str) -> Option<[bool; 3]> {
    let start = comment.find("pbc=\"")?;
    let rest = &comment[start + "pbc=\"".len()..];
    let end = rest.find('"')?;
    let values = rest[..end].split_whitespace().collect::<Vec<_>>();
    if values.len() != 3 {
        return None;
    }
    Some([
        values[0].eq_ignore_ascii_case("T") || values[0].eq_ignore_ascii_case("true"),
        values[1].eq_ignore_ascii_case("T") || values[1].eq_ignore_ascii_case("true"),
        values[2].eq_ignore_ascii_case("T") || values[2].eq_ignore_ascii_case("true"),
    ])
}

#[cfg(feature = "bridge-patina-types")]
pub fn candidate_from_xyz_path(path: &Path) -> Result<Candidate, XyzError> {
    let frames = read_xyz_frames(path)?;
    let frame = frames.into_iter().next().ok_or(XyzError::EmptyInput)?;
    let label = path
        .file_stem()
        .and_then(|stem| stem.to_str())
        .unwrap_or("xyz");
    candidate_from_xyz_frame(&frame, label)
}

#[cfg(feature = "bridge-patina-types")]
pub fn candidate_from_xyz_frame(
    frame: &XyzFrame,
    label: impl Into<String>,
) -> Result<Candidate, XyzError> {
    if frame.atom_count != frame.atoms.len() {
        return Err(XyzError::UnexpectedEof {
            expected_atoms: frame.atom_count,
            found_atoms: frame.atoms.len(),
        });
    }
    let candidate = Candidate::from_parts(
        label,
        frame
            .atoms
            .iter()
            .map(|atom| atom.species.clone())
            .collect(),
        frame.atoms.iter().map(|atom| atom.coords).collect(),
        None,
        [false, false, false],
    );
    candidate
        .validate()
        .map_err(|err| XyzError::InvalidCandidate {
            message: format!("{err:?}"),
        })?;
    Ok(candidate)
}

#[cfg(feature = "bridge-patina-types")]
pub fn candidate_from_extxyz_path(path: &Path) -> Result<Candidate, XyzError> {
    let frames = read_xyz_frames(path)?;
    let frame = frames.into_iter().next().ok_or(XyzError::EmptyInput)?;
    let label = path
        .file_stem()
        .and_then(|stem| stem.to_str())
        .unwrap_or("candidate");
    candidate_from_extxyz_frame(&frame, label)
}

#[cfg(feature = "bridge-patina-types")]
pub fn candidate_from_extxyz_frame(
    frame: &XyzFrame,
    label: impl Into<String>,
) -> Result<Candidate, XyzError> {
    if frame.atom_count != frame.atoms.len() {
        return Err(XyzError::UnexpectedEof {
            expected_atoms: frame.atom_count,
            found_atoms: frame.atoms.len(),
        });
    }

    let lattice = parse_extxyz_lattice(&frame.comment)?;
    let periodic_axes = parse_extxyz_pbc(&frame.comment).unwrap_or([
        lattice.is_some(),
        lattice.is_some(),
        lattice.is_some(),
    ]);

    let fractional_coords = if let Some(lattice) = lattice {
        frame
            .atoms
            .iter()
            .map(|atom| {
                cartesian_to_fractional(atom.coords, lattice)
                    .map(|coord| normalize_fractional_coordinate_with_axes(coord, periodic_axes))
            })
            .collect::<Result<Vec<_>, XyzError>>()?
    } else {
        frame.atoms.iter().map(|atom| atom.coords).collect()
    };

    let candidate = Candidate::from_parts(
        label,
        frame
            .atoms
            .iter()
            .map(|atom| atom.species.clone())
            .collect(),
        fractional_coords,
        lattice,
        periodic_axes,
    );
    candidate
        .validate()
        .map_err(|err| XyzError::InvalidCandidate {
            message: format!("{err:?}"),
        })?;
    Ok(candidate)
}

#[cfg(feature = "bridge-patina-types")]
pub fn write_candidate_xyz(
    candidate: &Candidate,
    path: &Path,
    options: &XyzEncodeOptions,
) -> Result<(), XyzError> {
    let rendered = encode_candidate_xyz(candidate, options)?;
    fs::write(path, rendered).map_err(|err| XyzError::Io {
        message: format!("failed to write `{}`: {err}", path.display()),
    })
}

#[cfg(feature = "bridge-patina-types")]
pub fn encode_candidate_xyz(
    candidate: &Candidate,
    options: &XyzEncodeOptions,
) -> Result<String, XyzError> {
    candidate
        .validate()
        .map_err(|err| XyzError::InvalidCandidate {
            message: format!("{err:?}"),
        })?;

    let coords = match options.coordinate_mode {
        XyzCoordinateMode::Stored => candidate.fractional_coords.clone(),
        XyzCoordinateMode::CartesianFromLattice => match candidate.lattice {
            Some(lattice) => candidate
                .fractional_coords
                .iter()
                .map(|coord| fractional_to_cartesian(*coord, lattice))
                .collect(),
            None => candidate.fractional_coords.clone(),
        },
    };

    let comment = if options.extxyz_fields {
        extxyz_comment(
            candidate.lattice,
            candidate.periodic_axes,
            options.label.as_deref().or(Some(candidate.label.as_str())),
            options.energy,
        )
    } else if let Some(comment) = &options.comment {
        comment.clone()
    } else if let Some(energy) = options.energy {
        format!(
            "label={} energy={energy:.10}",
            options.label.as_deref().unwrap_or(&candidate.label)
        )
    } else if let Some(label) = &options.label {
        format!("label={label}")
    } else {
        String::new()
    };

    let mut rendered = String::new();
    writeln!(&mut rendered, "{}", candidate.len()).expect("string write");
    writeln!(&mut rendered, "{comment}").expect("string write");
    for (species, coord) in candidate.species.iter().zip(coords.iter()) {
        writeln!(
            &mut rendered,
            "{species} {:.10} {:.10} {:.10}",
            coord[0], coord[1], coord[2]
        )
        .expect("string write");
    }
    Ok(rendered)
}

#[cfg(feature = "bridge-patina-types")]
fn extxyz_comment(
    lattice: Option<[[f64; 3]; 3]>,
    periodic_axes: [bool; 3],
    label: Option<&str>,
    energy: Option<f64>,
) -> String {
    let mut comment = String::new();
    if let Some(lattice) = lattice {
        write!(
            &mut comment,
            "Lattice=\"{:.10} {:.10} {:.10} {:.10} {:.10} {:.10} {:.10} {:.10} {:.10}\" ",
            lattice[0][0],
            lattice[0][1],
            lattice[0][2],
            lattice[1][0],
            lattice[1][1],
            lattice[1][2],
            lattice[2][0],
            lattice[2][1],
            lattice[2][2]
        )
        .expect("string write");
    }
    write!(
        &mut comment,
        "Properties=species:S:1:pos:R:3 pbc=\"{} {} {}\"",
        if periodic_axes[0] { "T" } else { "F" },
        if periodic_axes[1] { "T" } else { "F" },
        if periodic_axes[2] { "T" } else { "F" },
    )
    .expect("string write");
    if let Some(label) = label {
        write!(&mut comment, " label={label}").expect("string write");
    }
    if let Some(energy) = energy {
        write!(&mut comment, " energy={energy:.10}").expect("string write");
    }
    comment
}

#[cfg(feature = "bridge-patina-types")]
fn fractional_to_cartesian(coord: [f64; 3], lattice: [[f64; 3]; 3]) -> [f64; 3] {
    [
        coord[0] * lattice[0][0] + coord[1] * lattice[1][0] + coord[2] * lattice[2][0],
        coord[0] * lattice[0][1] + coord[1] * lattice[1][1] + coord[2] * lattice[2][1],
        coord[0] * lattice[0][2] + coord[1] * lattice[1][2] + coord[2] * lattice[2][2],
    ]
}

#[cfg(feature = "bridge-patina-types")]
fn cartesian_to_fractional(coord: [f64; 3], lattice: [[f64; 3]; 3]) -> Result<[f64; 3], XyzError> {
    let inverse = invert_lattice(lattice).ok_or_else(|| XyzError::InvalidCandidate {
        message: "failed to invert lattice while decoding extxyz coordinates".into(),
    })?;
    Ok([
        inverse[0][0] * coord[0] + inverse[0][1] * coord[1] + inverse[0][2] * coord[2],
        inverse[1][0] * coord[0] + inverse[1][1] * coord[1] + inverse[1][2] * coord[2],
        inverse[2][0] * coord[0] + inverse[2][1] * coord[1] + inverse[2][2] * coord[2],
    ])
}

#[cfg(feature = "bridge-patina-types")]
fn invert_lattice(lattice: [[f64; 3]; 3]) -> Option<[[f64; 3]; 3]> {
    let det = lattice[0][0] * (lattice[1][1] * lattice[2][2] - lattice[1][2] * lattice[2][1])
        - lattice[0][1] * (lattice[1][0] * lattice[2][2] - lattice[1][2] * lattice[2][0])
        + lattice[0][2] * (lattice[1][0] * lattice[2][1] - lattice[1][1] * lattice[2][0]);
    if det.abs() <= 1.0e-14 {
        return None;
    }
    let inv_det = 1.0 / det;
    Some([
        [
            (lattice[1][1] * lattice[2][2] - lattice[1][2] * lattice[2][1]) * inv_det,
            (lattice[0][2] * lattice[2][1] - lattice[0][1] * lattice[2][2]) * inv_det,
            (lattice[0][1] * lattice[1][2] - lattice[0][2] * lattice[1][1]) * inv_det,
        ],
        [
            (lattice[1][2] * lattice[2][0] - lattice[1][0] * lattice[2][2]) * inv_det,
            (lattice[0][0] * lattice[2][2] - lattice[0][2] * lattice[2][0]) * inv_det,
            (lattice[0][2] * lattice[1][0] - lattice[0][0] * lattice[1][2]) * inv_det,
        ],
        [
            (lattice[1][0] * lattice[2][1] - lattice[1][1] * lattice[2][0]) * inv_det,
            (lattice[0][1] * lattice[2][0] - lattice[0][0] * lattice[2][1]) * inv_det,
            (lattice[0][0] * lattice[1][1] - lattice[0][1] * lattice[1][0]) * inv_det,
        ],
    ])
}

#[cfg(feature = "bridge-patina-types")]
fn normalize_fractional_coordinate_with_axes(
    coord: [f64; 3],
    periodic_axes: [bool; 3],
) -> [f64; 3] {
    let mut normalized = coord;
    for axis in 0..3 {
        if periodic_axes[axis] {
            normalized[axis] = normalized[axis].rem_euclid(1.0);
        }
    }
    normalized
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_plain_xyz_frame() {
        let frames = parse_xyz_frames("2\ncomment\nMg 0 0 0\nO 1 0 0\n").expect("frames");
        assert_eq!(frames.len(), 1);
        assert_eq!(frames[0].atom_count, 2);
        assert_eq!(frames[0].atoms[1].species, "O");
    }

    #[test]
    fn parses_extxyz_comment_metadata() {
        let lattice = parse_extxyz_lattice(
            "Lattice=\"1.0, 0.0, 0.0 0.5, 2.0, 0.0 0.0, 0.0, 3.0\" pbc=\"T F T\"",
        )
        .expect("lattice")
        .expect("present");
        assert_eq!(lattice[0], [1.0, 0.0, 0.0]);
        assert_eq!(
            parse_extxyz_pbc("Lattice=\"1 0 0 0 1 0 0 0 1\" pbc=\"T F T\""),
            Some([true, false, true])
        );
    }

    #[cfg(feature = "bridge-patina-types")]
    #[test]
    fn encodes_candidate_to_extxyz() {
        let candidate = Candidate::fully_periodic(
            "bulk",
            vec!["Mg".into()],
            vec![[0.5, 0.5, 0.5]],
            [[4.0, 0.0, 0.0], [0.0, 4.0, 0.0], [0.0, 0.0, 4.0]],
        );
        let rendered = encode_candidate_xyz(
            &candidate,
            &XyzEncodeOptions {
                coordinate_mode: XyzCoordinateMode::CartesianFromLattice,
                extxyz_fields: true,
                label: Some(candidate.label.clone()),
                energy: None,
                comment: None,
            },
        )
        .expect("xyz");
        assert!(rendered.contains("Lattice=\"4.0000000000"));
        assert!(rendered.contains("pbc=\"T T T\""));
        assert!(rendered.contains("Mg 2.0000000000 2.0000000000 2.0000000000"));
    }

    #[test]
    fn writes_plain_xyz_frames() {
        let rendered = encode_xyz_frames(&[XyzFrame {
            atom_count: 1,
            comment: "frame".into(),
            atoms: vec![AtomRecord {
                species: "Mg".into(),
                coords: [0.0, 0.0, 0.0],
            }],
        }])
        .expect("xyz");
        assert!(rendered.starts_with("1\nframe\nMg "));
    }

    #[cfg(feature = "bridge-patina-types")]
    #[test]
    fn decodes_partial_periodic_extxyz_candidate() {
        let candidate = candidate_from_extxyz_frame(
            &XyzFrame {
                atom_count: 2,
                comment: "Lattice=\"2 0 0 0 20 0 0 0 20\" pbc=\"T F F\"".into(),
                atoms: vec![
                    AtomRecord {
                        species: "Mg".into(),
                        coords: [0.1, 0.0, 0.0],
                    },
                    AtomRecord {
                        species: "O".into(),
                        coords: [1.9, 0.0, 0.0],
                    },
                ],
            },
            "wire",
        )
        .expect("candidate");
        assert_eq!(candidate.periodic_axes, [true, false, false]);
        assert!((candidate.fractional_coords[0][0] - 0.05).abs() < 1.0e-12);
        assert!((candidate.fractional_coords[1][0] - 0.95).abs() < 1.0e-12);
    }
}
