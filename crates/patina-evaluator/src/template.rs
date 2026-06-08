use std::fs;

use camino::{Utf8Path, Utf8PathBuf};
use indexmap::IndexMap;
use serde::{Deserialize, Serialize};
use thiserror::Error;

/// High-level structured view of the native Scott master template.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct MasterTemplateLayout {
    pub sections: IndexMap<MasterTemplateSection, Vec<String>>,
}

/// Named logical regions of a native Scott master template.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MasterTemplateSection {
    Header,
    AtomBlock,
    LocalAtomBlock,
    PotentialBlock,
    SecondPotentialBlock,
}

/// Compact parse summary used while the full parser is still being migrated.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct MasterTemplateSummary {
    pub atom_count: usize,
    pub has_second_stage_block: bool,
    pub has_shells_first_stage: bool,
    pub has_shells_second_stage: bool,
}

/// Errors surfaced by the future structured `Master.gin` parser.
#[derive(Debug, Error)]
pub enum MasterTemplateParseError {
    #[error("failed to read master template from `{path}`")]
    Io {
        path: Utf8PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error("missing expected master template marker or structure section: {0}")]
    MissingStructureMarker(String),
    #[error("invalid cell or vector definition: {0}")]
    InvalidCellDefinition(String),
    #[error("invalid atom row: {0}")]
    InvalidAtomRow(String),
}

impl MasterTemplateLayout {
    pub fn from_path(path: &Utf8Path) -> Result<Self, MasterTemplateParseError> {
        let text = fs::read_to_string(path).map_err(|source| MasterTemplateParseError::Io {
            path: path.to_path_buf(),
            source,
        })?;
        Self::parse(&text)
    }

    pub fn parse(text: &str) -> Result<Self, MasterTemplateParseError> {
        let lines: Vec<String> = text
            .lines()
            .map(str::trim)
            .filter(|line| !line.is_empty())
            .map(ToOwned::to_owned)
            .collect();

        let structure_start = lines
            .iter()
            .position(|line| is_structure_marker(line))
            .ok_or_else(|| {
                MasterTemplateParseError::MissingStructureMarker(
                    "expected `cartesian` or `fractional` block".into(),
                )
            })?;

        validate_vectors_block(&lines[..structure_start])?;

        let mut sections = IndexMap::new();
        let header = lines[..structure_start].to_vec();
        if !header.is_empty() {
            sections.insert(MasterTemplateSection::Header, header);
        }

        let mut atom_block = Vec::new();
        let mut local_atom_block = Vec::new();
        let mut potential_block = Vec::new();
        let mut second_potential_block = Vec::new();

        enum ActiveSection {
            Atom,
            LocalAtom,
            Potential,
            SecondPotential,
        }

        let mut active = ActiveSection::Atom;
        for line in lines.iter().skip(structure_start) {
            if is_comment(line) {
                continue;
            }

            if is_structure_marker(line) {
                if matches!(active, ActiveSection::Atom) && !atom_block.is_empty() {
                    active = ActiveSection::LocalAtom;
                }
                match active {
                    ActiveSection::Atom => atom_block.push(line.clone()),
                    ActiveSection::LocalAtom => local_atom_block.push(line.clone()),
                    ActiveSection::Potential => potential_block.push(line.clone()),
                    ActiveSection::SecondPotential => second_potential_block.push(line.clone()),
                }
                continue;
            }

            if matches!(active, ActiveSection::Atom | ActiveSection::LocalAtom) && is_atom_row(line)
            {
                match active {
                    ActiveSection::Atom => atom_block.push(line.clone()),
                    ActiveSection::LocalAtom => local_atom_block.push(line.clone()),
                    _ => unreachable!(),
                }
                continue;
            }

            if matches!(active, ActiveSection::Atom | ActiveSection::LocalAtom)
                && is_atom_block_directive(line)
            {
                match active {
                    ActiveSection::Atom => atom_block.push(line.clone()),
                    ActiveSection::LocalAtom => local_atom_block.push(line.clone()),
                    _ => unreachable!(),
                }
                continue;
            }

            if matches!(active, ActiveSection::Atom | ActiveSection::LocalAtom)
                && !looks_like_potential_line(line)
            {
                return Err(MasterTemplateParseError::InvalidAtomRow(line.clone()));
            }

            if matches!(active, ActiveSection::Potential)
                && starts_second_potential_block(line)
                && !potential_block.is_empty()
            {
                active = ActiveSection::SecondPotential;
            } else if matches!(active, ActiveSection::Atom | ActiveSection::LocalAtom) {
                active = ActiveSection::Potential;
            }

            match active {
                ActiveSection::Potential => potential_block.push(line.clone()),
                ActiveSection::SecondPotential => second_potential_block.push(line.clone()),
                _ => unreachable!(),
            }
        }

        if atom_block.is_empty() {
            return Err(MasterTemplateParseError::MissingStructureMarker(
                "structure marker was found but no atom rows were parsed".into(),
            ));
        }

        sections.insert(MasterTemplateSection::AtomBlock, atom_block);
        if !local_atom_block.is_empty() {
            sections.insert(MasterTemplateSection::LocalAtomBlock, local_atom_block);
        }
        if !potential_block.is_empty() {
            sections.insert(MasterTemplateSection::PotentialBlock, potential_block);
        }
        if !second_potential_block.is_empty() {
            sections.insert(
                MasterTemplateSection::SecondPotentialBlock,
                second_potential_block,
            );
        }

        Ok(Self { sections })
    }

    pub fn summary(&self) -> MasterTemplateSummary {
        summarize_master_template(self)
    }
}

pub fn summarize_master_template(layout: &MasterTemplateLayout) -> MasterTemplateSummary {
    let atom_block = layout
        .sections
        .get(&MasterTemplateSection::AtomBlock)
        .map(Vec::as_slice)
        .unwrap_or(&[]);
    let potential_block = layout
        .sections
        .get(&MasterTemplateSection::PotentialBlock)
        .map(Vec::as_slice)
        .unwrap_or(&[]);
    let second_potential_block = layout
        .sections
        .get(&MasterTemplateSection::SecondPotentialBlock)
        .map(Vec::as_slice)
        .unwrap_or(&[]);

    MasterTemplateSummary {
        atom_count: atom_block.iter().filter(|line| is_atom_row(line)).count(),
        has_second_stage_block: !second_potential_block.is_empty(),
        has_shells_first_stage: section_has_shells(atom_block)
            || section_has_shells(potential_block),
        has_shells_second_stage: section_has_shells(second_potential_block),
    }
}

fn validate_vectors_block(lines: &[String]) -> Result<(), MasterTemplateParseError> {
    let Some(index) = lines
        .iter()
        .position(|line| line.trim().eq_ignore_ascii_case("vectors"))
    else {
        return Ok(());
    };

    let vector_rows: Vec<&str> = lines
        .iter()
        .skip(index + 1)
        .filter(|line| !is_comment(line))
        .take(3)
        .map(String::as_str)
        .collect();
    if vector_rows.len() != 3 {
        return Err(MasterTemplateParseError::InvalidCellDefinition(
            "expected three vector rows after `vectors`".into(),
        ));
    }

    for row in vector_rows {
        let fields: Vec<&str> = row.split_whitespace().collect();
        if fields.len() < 3
            || !fields
                .iter()
                .take(3)
                .all(|value| value.parse::<f64>().is_ok())
        {
            return Err(MasterTemplateParseError::InvalidCellDefinition(
                row.to_string(),
            ));
        }
    }

    Ok(())
}

fn is_comment(line: &str) -> bool {
    line.trim_start().starts_with('#')
}

fn is_structure_marker(line: &str) -> bool {
    let lowered = line.trim().to_ascii_lowercase();
    lowered == "cartesian"
        || lowered == "fractional"
        || lowered.starts_with("cartesian region ")
        || lowered.starts_with("fractional region ")
}

fn is_atom_row(line: &str) -> bool {
    let fields: Vec<&str> = line.split_whitespace().collect();
    if fields.len() < 4 {
        return false;
    }

    if fields[1].parse::<f64>().is_ok() {
        return fields.len() >= 4
            && fields[1..4]
                .iter()
                .all(|value| value.parse::<f64>().is_ok());
    }

    fields.len() >= 5
        && fields[2..5]
            .iter()
            .all(|value| value.parse::<f64>().is_ok())
}

fn looks_like_potential_line(line: &str) -> bool {
    let lowered = line.trim().to_ascii_lowercase();
    if lowered.is_empty() {
        return false;
    }
    let first = lowered.split_whitespace().next().unwrap_or_default();
    matches!(
        first,
        "species"
            | "buckingham"
            | "morse"
            | "spring"
            | "harmonic"
            | "lennard"
            | "three"
            | "threebody"
            | "sw2"
            | "reaxff"
            | "accuracy"
            | "xtol"
            | "gtol"
            | "ftol"
            | "switch"
            | "maxcyc"
            | "cutp"
            | "dispersion"
            | "stepmx"
    ) || first.parse::<f64>().is_ok()
}

fn is_atom_block_directive(line: &str) -> bool {
    let lowered = line.trim().to_ascii_lowercase();
    matches!(lowered.as_str(), "extra")
}

fn starts_second_potential_block(line: &str) -> bool {
    line.trim().eq_ignore_ascii_case("species")
}

fn section_has_shells(lines: &[String]) -> bool {
    lines.iter().any(|line| {
        line.split_whitespace()
            .any(|field| field.eq_ignore_ascii_case("shel"))
    })
}

#[cfg(test)]
mod tests {
    use super::{MasterTemplateLayout, MasterTemplateParseError, MasterTemplateSection};

    #[test]
    fn parses_cluster_template_and_builds_summary() {
        let layout = MasterTemplateLayout::parse(include_str!(
            "../tests/fixtures/master_cluster_fixture.gin"
        ))
        .unwrap();
        let summary = layout.summary();

        assert!(layout.sections.contains_key(&MasterTemplateSection::Header));
        assert!(layout
            .sections
            .contains_key(&MasterTemplateSection::AtomBlock));
        assert!(layout
            .sections
            .contains_key(&MasterTemplateSection::PotentialBlock));
        assert_eq!(summary.atom_count, 16);
        assert!(!summary.has_second_stage_block);
        assert!(!summary.has_shells_first_stage);
    }

    #[test]
    fn parses_periodic_template_with_vectors_and_shells() {
        let layout = MasterTemplateLayout::parse(include_str!(
            "../tests/fixtures/master_periodic_shell_fixture.gin"
        ))
        .unwrap();
        let summary = layout.summary();

        assert!(layout.sections.contains_key(&MasterTemplateSection::Header));
        assert_eq!(summary.atom_count, 4);
        assert!(summary.has_shells_first_stage);
    }

    #[test]
    fn detects_second_potential_stage_block() {
        let layout = MasterTemplateLayout::parse(
            r#"
            opti
            cartesian
            Mg core 0.0 0.0 0.0
            O core 1.0 1.0 1.0
            species
            Mg core 2.0
            O core -2.0
            buckingham
            Mg core O core 1.0 1.0 0.0
            species
            Mg core 2.0
            O shel -2.0
            spring
            O core 10.0 0.0
            "#,
        )
        .unwrap();

        let summary = layout.summary();
        assert!(summary.has_second_stage_block);
        assert!(summary.has_shells_second_stage);
    }

    #[test]
    fn rejects_templates_without_structure_markers() {
        let error = MasterTemplateLayout::parse("species\nMg core 2.0").unwrap_err();
        assert!(matches!(
            error,
            MasterTemplateParseError::MissingStructureMarker(_)
        ));
    }
}
