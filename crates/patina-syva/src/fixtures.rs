use crate::error::SyvaError;
use crate::model::{
    species_for_atomic_number, PointGroupLabel, SyvaInputAtom, SyvaInputGeometry, SyvaSubsetSpec,
};
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BundledSyvaFixture {
    pub name: &'static str,
    pub title: &'static str,
    pub analysis_focus: &'static [&'static str],
    pub output_variants: &'static [&'static str],
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SyvaFixturePaths {
    pub input: PathBuf,
    pub output: PathBuf,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SyvaFixtureEquivalenceClass {
    pub species: String,
    pub atom_indices: Vec<usize>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct SyvaFixtureOutputSummary {
    pub title: String,
    pub point_group: PointGroupLabel,
    pub framework_group: Option<String>,
    pub symmetry_equivalence_classes: Vec<SyvaFixtureEquivalenceClass>,
    pub operation_count: Option<usize>,
    pub max_deviation: Option<f64>,
    pub molecular_weight: Option<f64>,
    pub centre_of_mass: Option<[f64; 3]>,
    pub shifted_atoms: Vec<SyvaInputAtom>,
    pub used_subset: bool,
}

const BUNDLED_SYVA_FIXTURE_MANIFEST: &[BundledSyvaFixture] = &[
    BundledSyvaFixture {
        name: "C60",
        title: "Bucky Ball",
        analysis_focus: &[
            "icosahedral families",
            "optimization",
            "polyhedral subgroup coverage",
        ],
        output_variants: &[],
    },
    BundledSyvaFixture {
        name: "CO",
        title: "CO",
        analysis_focus: &["linear classification", "minimal two-atom input"],
        output_variants: &[],
    },
    BundledSyvaFixture {
        name: "CO2",
        title: "CO2",
        analysis_focus: &[
            "linear full-group handling",
            "equivalence classes",
            "Dih families",
        ],
        output_variants: &[],
    },
    BundledSyvaFixture {
        name: "CaTHF6",
        title: "[Ca(THF)6]2+",
        analysis_focus: &[
            "subset-mode preprocessing",
            "high-symmetry framework grouping",
        ],
        output_variants: &["CaTHF6_subset"],
    },
    BundledSyvaFixture {
        name: "H2O2",
        title: "H2O2",
        analysis_focus: &["rotation-axis search", "small non-linear molecule"],
        output_variants: &[],
    },
    BundledSyvaFixture {
        name: "N4S4",
        title: "N4S4",
        analysis_focus: &["improper rotations", "dihedral-family optimization"],
        output_variants: &[],
    },
    BundledSyvaFixture {
        name: "benzene",
        title: "Benzene",
        analysis_focus: &[
            "normalization parity",
            "planarity",
            "baseline parser coverage",
        ],
        output_variants: &["benzene_tol"],
    },
    BundledSyvaFixture {
        name: "cubane",
        title: "Cubane",
        analysis_focus: &[
            "cubic symmetry",
            "operation summaries",
            "subgroup selection",
        ],
        output_variants: &[],
    },
    BundledSyvaFixture {
        name: "neopentane",
        title: "Neopentane",
        analysis_focus: &[
            "improper rotation families",
            "tetrahedral subgroup coverage",
        ],
        output_variants: &[],
    },
    BundledSyvaFixture {
        name: "propyne",
        title: "Propyne",
        analysis_focus: &["principal-axis cyclic families", "subgroup optimization"],
        output_variants: &[],
    },
    BundledSyvaFixture {
        name: "tcpropmethane",
        title: "Tetracyclopropylmethane",
        analysis_focus: &["subset-aware output parsing", "legacy upstream edge case"],
        output_variants: &["tcpropmethane_subset"],
    },
];

fn bundled_fixture_root() -> PathBuf {
    let crate_dir = Path::new(env!("CARGO_MANIFEST_DIR"));
    let legacy_root = crate_dir.join("../../Analysis-Toolkit-master/SYVA/test");
    if legacy_root.exists() {
        return legacy_root;
    }

    crate_dir.join("../../extra/to_integrate_project/Analysis-Toolkit-master/SYVA/test")
}

pub fn bundled_fixture_manifest() -> &'static [BundledSyvaFixture] {
    BUNDLED_SYVA_FIXTURE_MANIFEST
}

pub fn bundled_fixture(name: &str) -> Option<&'static BundledSyvaFixture> {
    bundled_fixture_manifest()
        .iter()
        .find(|fixture| fixture.name == name)
}

pub fn bundled_fixture_output_path(name: &str) -> PathBuf {
    bundled_fixture_root().join("output").join(name)
}

pub fn bundled_fixture_paths(name: &str) -> SyvaFixturePaths {
    SyvaFixturePaths {
        input: bundled_fixture_root().join("input").join(name),
        output: bundled_fixture_output_path(name),
    }
}

pub fn load_fixture_input(path: &Path) -> Result<SyvaInputGeometry, SyvaError> {
    let text = fs::read_to_string(path).map_err(|error| {
        SyvaError::InvalidFixtureInput(format!("failed to read `{}`: {error}", path.display()))
    })?;
    parse_fixture_input(&text)
}

pub fn load_fixture_output_summary(path: &Path) -> Result<SyvaFixtureOutputSummary, SyvaError> {
    let text = fs::read_to_string(path).map_err(|error| {
        SyvaError::InvalidFixtureOutput(format!("failed to read `{}`: {error}", path.display()))
    })?;
    parse_fixture_output_summary(&text)
}

pub fn parse_fixture_input(text: &str) -> Result<SyvaInputGeometry, SyvaError> {
    let mut lines = text.lines().filter(|line| !line.trim().is_empty());
    let title = lines
        .next()
        .ok_or_else(|| SyvaError::InvalidFixtureInput("missing title line".into()))?
        .trim()
        .to_string();
    let atom_count = parse_usize_input(
        lines
            .next()
            .ok_or_else(|| SyvaError::InvalidFixtureInput("missing atom count line".into()))?,
        "atom count",
    )?;

    let mut atoms = Vec::with_capacity(atom_count);
    for atom_index in 0..atom_count {
        let line = lines.next().ok_or_else(|| {
            SyvaError::InvalidFixtureInput(format!("missing atom line {atom_index}"))
        })?;
        let parts = line.split_whitespace().collect::<Vec<_>>();
        if parts.len() != 4 {
            return Err(SyvaError::InvalidFixtureInput(format!(
                "atom line {atom_index} must contain atomic number and three coordinates"
            )));
        }

        let atomic_number = parse_u8_input(parts[0], "atomic number")?;
        let species = species_for_atomic_number(atomic_number)
            .ok_or(SyvaError::UnsupportedAtomicNumber { atomic_number })?
            .to_string();
        atoms.push(SyvaInputAtom {
            atomic_number,
            species,
            cartesian: [
                parse_f64_input(parts[1], "x coordinate")?,
                parse_f64_input(parts[2], "y coordinate")?,
                parse_f64_input(parts[3], "z coordinate")?,
            ],
        });
    }

    let remaining = lines
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .collect::<Vec<_>>();
    let subset = if remaining.is_empty() {
        None
    } else {
        let atom_count = parse_usize_input(remaining[0], "subset atom count")?;
        let atom_indices = remaining[1..]
            .iter()
            .flat_map(|line| line.split_whitespace())
            .map(|value| parse_usize_input(value, "subset atom index"))
            .collect::<Result<Vec<_>, _>>()?;
        if atom_indices.len() != atom_count {
            return Err(SyvaError::InvalidFixtureInput(format!(
                "subset atom count {atom_count} does not match {} indices",
                atom_indices.len()
            )));
        }
        Some(SyvaSubsetSpec {
            atom_count,
            atom_indices,
        })
    };

    Ok(SyvaInputGeometry {
        title,
        atoms,
        subset,
    })
}

pub fn parse_fixture_output_summary(text: &str) -> Result<SyvaFixtureOutputSummary, SyvaError> {
    let title = extract_value_after_prefix(text, "-- Title:")
        .ok_or_else(|| SyvaError::InvalidFixtureOutput("missing title header".into()))?;
    let point_group_line = text
        .lines()
        .find(|line| line.contains("point group."))
        .ok_or_else(|| SyvaError::InvalidFixtureOutput("missing point-group summary".into()))?;
    let point_group = point_group_line
        .split_whitespace()
        .collect::<Vec<_>>()
        .windows(2)
        .find_map(|window| (window[1] == "point").then_some(window[0]))
        .ok_or_else(|| SyvaError::InvalidFixtureOutput("could not parse point-group label".into()))
        .and_then(PointGroupLabel::new)?;
    let framework_group = extract_value_after_prefix(text, "Framework group:");
    let symmetry_equivalence_classes = parse_equivalence_classes(text)?;
    let operation_count = text
        .lines()
        .find(|line| line.contains("-- Number of symmetry operations (including E) ="))
        .and_then(|line| line.split('=').nth(1))
        .map(str::trim)
        .map(|value| parse_usize_output(value, "operation count"))
        .transpose()?;
    let max_deviation = text
        .lines()
        .find(|line| line.contains("Distorsion of geometry due to symmetry elements:"))
        .and_then(|line| line.split(':').nth(1))
        .map(str::trim)
        .map(|value| parse_f64_output(value, "geometry distortion"))
        .transpose()?;
    let molecular_weight = text
        .lines()
        .find(|line| line.contains("-- Molecular weight="))
        .and_then(|line| line.split('=').nth(1))
        .map(str::trim)
        .map(|value| parse_f64_output(value, "molecular weight"))
        .transpose()?;
    let centre_of_mass = parse_centre_of_mass(text)?;
    let shifted_atoms = parse_shifted_atoms(text)?;
    let used_subset = text
        .lines()
        .any(|line| line.contains("Using only the requested subset"));

    Ok(SyvaFixtureOutputSummary {
        title,
        point_group,
        framework_group,
        symmetry_equivalence_classes,
        operation_count,
        max_deviation,
        molecular_weight,
        centre_of_mass,
        shifted_atoms,
        used_subset,
    })
}

fn parse_equivalence_classes(text: &str) -> Result<Vec<SyvaFixtureEquivalenceClass>, SyvaError> {
    let mut classes = Vec::<SyvaFixtureEquivalenceClass>::new();
    let mut lines = text.lines().peekable();

    while let Some(line) = lines.next() {
        if !line.contains("-- Symmetry-equivalence classes of atoms:") {
            continue;
        }

        while let Some(next) = lines.next() {
            let trimmed = next.trim();
            if trimmed.is_empty() {
                continue;
            }
            if trimmed.starts_with("Distorsion") || trimmed.starts_with("-- ") {
                break;
            }
            if !trimmed.starts_with('#') {
                continue;
            }

            let species = trimmed
                .split("(atom")
                .nth(1)
                .and_then(|tail| tail.split(')').next())
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .ok_or_else(|| {
                    SyvaError::InvalidFixtureOutput(format!(
                        "failed to parse symmetry-equivalence species from `{trimmed}`"
                    ))
                })?
                .to_string();

            let atom_indices = loop {
                let Some(candidate) = lines.next() else {
                    return Err(SyvaError::InvalidFixtureOutput(
                        "missing symmetry-equivalence atom indices".into(),
                    ));
                };
                let candidate = candidate.trim();
                if candidate.is_empty() {
                    continue;
                }
                break candidate
                    .split_whitespace()
                    .map(|value| parse_usize_output(value, "symmetry-equivalence atom index"))
                    .collect::<Result<Vec<_>, _>>()?;
            };

            classes.push(SyvaFixtureEquivalenceClass {
                species,
                atom_indices,
            });
        }

        break;
    }

    Ok(classes)
}

fn parse_centre_of_mass(text: &str) -> Result<Option<[f64; 3]>, SyvaError> {
    let line = match text.lines().find(|line| line.contains("cmx=")) {
        Some(line) => line,
        None => return Ok(None),
    };

    let parts = line.split_whitespace().collect::<Vec<_>>();
    if parts.len() < 6 {
        return Err(SyvaError::InvalidFixtureOutput(
            "centre-of-mass line is malformed".into(),
        ));
    }

    Ok(Some([
        parse_f64_output(parts[1], "cmx")?,
        parse_f64_output(parts[3], "cmy")?,
        parse_f64_output(parts[5], "cmz")?,
    ]))
}

fn parse_shifted_atoms(text: &str) -> Result<Vec<SyvaInputAtom>, SyvaError> {
    let mut shifted_atoms = Vec::new();
    let mut capture = false;

    for line in text.lines() {
        let trimmed = line.trim();
        if trimmed == "-- Cartesian coordinates related to centre of mass --" {
            capture = true;
            continue;
        }

        if !capture {
            continue;
        }

        if trimmed.starts_with("-- ") && !shifted_atoms.is_empty() {
            break;
        }
        if trimmed.is_empty() {
            continue;
        }

        let parts = trimmed.split_whitespace().collect::<Vec<_>>();
        if parts.len() != 4 {
            continue;
        }

        let Ok(atomic_number) = parts[0].parse::<u8>() else {
            continue;
        };

        let species = species_for_atomic_number(atomic_number)
            .ok_or(SyvaError::UnsupportedAtomicNumber { atomic_number })?
            .to_string();
        shifted_atoms.push(SyvaInputAtom {
            atomic_number,
            species,
            cartesian: [
                parse_f64_output(parts[1], "shifted x coordinate")?,
                parse_f64_output(parts[2], "shifted y coordinate")?,
                parse_f64_output(parts[3], "shifted z coordinate")?,
            ],
        });
    }

    Ok(shifted_atoms)
}

fn extract_value_after_prefix(text: &str, prefix: &str) -> Option<String> {
    text.lines()
        .find_map(|line| line.trim_start().strip_prefix(prefix).map(str::trim))
        .map(ToOwned::to_owned)
        .filter(|value| !value.is_empty())
}

fn parse_usize_input(raw: &str, label: &str) -> Result<usize, SyvaError> {
    raw.trim().parse::<usize>().map_err(|_| {
        SyvaError::InvalidFixtureInput(format!("failed to parse {label} from `{raw}`"))
    })
}

fn parse_u8_input(raw: &str, label: &str) -> Result<u8, SyvaError> {
    raw.trim().parse::<u8>().map_err(|_| {
        SyvaError::InvalidFixtureInput(format!("failed to parse {label} from `{raw}`"))
    })
}

fn parse_f64_input(raw: &str, label: &str) -> Result<f64, SyvaError> {
    raw.trim().parse::<f64>().map_err(|_| {
        SyvaError::InvalidFixtureInput(format!("failed to parse {label} from `{raw}`"))
    })
}

fn parse_usize_output(raw: &str, label: &str) -> Result<usize, SyvaError> {
    raw.trim().parse::<usize>().map_err(|_| {
        SyvaError::InvalidFixtureOutput(format!("failed to parse {label} from `{raw}`"))
    })
}

fn parse_f64_output(raw: &str, label: &str) -> Result<f64, SyvaError> {
    raw.trim().parse::<f64>().map_err(|_| {
        SyvaError::InvalidFixtureOutput(format!("failed to parse {label} from `{raw}`"))
    })
}

#[cfg(test)]
mod tests {
    use super::{
        bundled_fixture_manifest, bundled_fixture_output_path, bundled_fixture_paths,
        load_fixture_input, load_fixture_output_summary, parse_fixture_input,
        parse_fixture_output_summary,
    };
    use std::collections::BTreeSet;
    use std::fs;

    #[test]
    fn parses_basic_fixture_input() {
        let fixture = parse_fixture_input("Benzene\n2\n6 0.0 0.0 0.0\n1 1.0 0.0 0.0\n")
            .expect("fixture input");
        assert_eq!(fixture.title, "Benzene");
        assert_eq!(fixture.atoms.len(), 2);
        assert_eq!(fixture.atoms[0].species, "C");
        assert!(fixture.subset.is_none());
    }

    #[test]
    fn parses_fixture_input_with_subset() {
        let fixture = parse_fixture_input("Subset\n2\n20 0.0 0.0 0.0\n8 1.0 0.0 0.0\n2\n1 2\n")
            .expect("fixture input");
        let subset = fixture.subset.expect("subset");
        assert_eq!(subset.atom_count, 2);
        assert_eq!(subset.atom_indices, vec![1, 2]);
    }

    #[test]
    fn parses_fixture_input_with_multiline_subset_indices() {
        let fixture = parse_fixture_input(
            "Subset\n3\n20 0.0 0.0 0.0\n8 1.0 0.0 0.0\n8 -1.0 0.0 0.0\n3\n1 2\n3\n",
        )
        .expect("fixture input");
        let subset = fixture.subset.expect("subset");
        assert_eq!(subset.atom_count, 3);
        assert_eq!(subset.atom_indices, vec![1, 2, 3]);
    }

    #[test]
    fn loads_bundled_fixture_input_and_output_summary() {
        let benzene = bundled_fixture_paths("benzene");
        let input = load_fixture_input(&benzene.input).expect("benzene input");
        let output = load_fixture_output_summary(&benzene.output).expect("benzene output");
        assert_eq!(input.title, "Benzene");
        assert_eq!(output.title, "Benzene");
        assert_eq!(output.point_group.as_str(), "C1");
        assert_eq!(output.operation_count, Some(1));
        assert_eq!(output.molecular_weight, Some(78.046980));
        assert_eq!(output.shifted_atoms.len(), 12);
    }

    #[test]
    fn parses_subset_usage_from_fixture_output() {
        let output = load_fixture_output_summary(&bundled_fixture_output_path("CaTHF6_subset"))
            .expect("CaTHF6 subset output");
        assert!(output.used_subset);
        assert_eq!(output.point_group.as_str(), "Oh");
        assert_eq!(output.shifted_atoms.len(), 7);
    }

    #[test]
    fn parses_fixture_output_summary() {
        let summary = parse_fixture_output_summary(
            "-- Title: Example\n-- Molecular weight=        18.015660\n\n-- Centre of mass\n\n   cmx=      0.000000    cmy=     0.000000    cmz=     0.100000\n\n-- Cartesian coordinates related to centre of mass --\n  8          0.000000        0.000000       -0.100000\n  1          0.700000        0.000000        0.400000\n-- Number of symmetry operations (including E) =     8\n     Distorsion of geometry due to symmetry elements:   0.00000000\n-- The structure should belong to the C2v point group.\n     Framework group: C2v[X]\n",
        )
        .expect("fixture output");
        assert_eq!(summary.title, "Example");
        assert_eq!(summary.point_group.as_str(), "C2v");
        assert_eq!(summary.operation_count, Some(8));
        assert_eq!(summary.framework_group.as_deref(), Some("C2v[X]"));
        assert!(summary.symmetry_equivalence_classes.is_empty());
        assert_eq!(summary.molecular_weight, Some(18.015660));
        assert_eq!(summary.centre_of_mass, Some([0.0, 0.0, 0.1]));
        assert_eq!(summary.shifted_atoms.len(), 2);
    }

    #[test]
    fn parses_symmetry_equivalence_classes_from_fixture_output() {
        let summary = load_fixture_output_summary(&bundled_fixture_paths("CO2").output)
            .expect("fixture output");
        assert_eq!(summary.symmetry_equivalence_classes.len(), 2);
        assert_eq!(summary.symmetry_equivalence_classes[0].species, "O");
        assert_eq!(
            summary.symmetry_equivalence_classes[0].atom_indices,
            vec![1, 3]
        );
        assert_eq!(summary.symmetry_equivalence_classes[1].species, "C");
        assert_eq!(
            summary.symmetry_equivalence_classes[1].atom_indices,
            vec![2]
        );
    }

    #[test]
    fn bundled_fixture_manifest_covers_all_bundled_fixture_inputs_and_outputs() {
        let root = super::bundled_fixture_root();
        let input_names = fs::read_dir(root.join("input"))
            .expect("input dir")
            .map(|entry| {
                entry
                    .expect("input entry")
                    .file_name()
                    .to_string_lossy()
                    .to_string()
            })
            .collect::<BTreeSet<_>>();
        let output_names = fs::read_dir(root.join("output"))
            .expect("output dir")
            .map(|entry| {
                entry
                    .expect("output entry")
                    .file_name()
                    .to_string_lossy()
                    .to_string()
            })
            .collect::<BTreeSet<_>>();

        let manifest_input_names = bundled_fixture_manifest()
            .iter()
            .map(|fixture| fixture.name.to_string())
            .collect::<BTreeSet<_>>();
        let manifest_output_names = bundled_fixture_manifest()
            .iter()
            .flat_map(|fixture| {
                std::iter::once(fixture.name).chain(fixture.output_variants.iter().copied())
            })
            .map(str::to_string)
            .collect::<BTreeSet<_>>();

        assert_eq!(manifest_input_names, input_names);
        assert_eq!(manifest_output_names, output_names);
    }

    #[test]
    fn bundled_fixture_manifest_entries_resolve_and_parse() {
        for fixture in bundled_fixture_manifest() {
            let paths = bundled_fixture_paths(fixture.name);
            let input = load_fixture_input(&paths.input).expect("fixture input");
            let output = load_fixture_output_summary(&paths.output).expect("fixture output");

            assert_eq!(input.title.trim(), fixture.title);
            assert_eq!(output.title.trim(), fixture.title);

            for variant in fixture.output_variants {
                let summary = load_fixture_output_summary(&bundled_fixture_output_path(variant))
                    .expect("fixture output variant");
                assert_eq!(summary.title.trim(), fixture.title);
            }
        }
    }
}
