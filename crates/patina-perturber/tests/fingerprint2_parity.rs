use patina_perturber::{
    AssignmentFingerprintDistanceEngine, EnvironmentOverlapFingerprintConfig,
    EnvironmentOverlapFingerprintEngine, EnvironmentStructureFingerprintEngine,
};
use patina_sci_kernel::{
    Cluster0D, CoordinateBasis, Framework3D, Lattice3, Site, StructureMetadata,
};
use std::fs;
use std::path::Path;

const FIXTURES_DIR: &str = "tests/fixtures/fingerprint2";
const ANGSTROM_TO_BOHR: f64 = 1.889_726_125;
const FINGERPRINT2_WIDTH_CUTOFF_BOHR: f64 = 5.0;
const PARITY_TOLERANCE: f64 = 5.0e-8;
const DISTANCE_PARITY_TOLERANCE: f64 = 5.0e-5;

// Provenance:
// - Reference behavior captured from `Fingerprint2/src/fp_for_clusters.f90` and
//   `Fingerprint2/src/fp_for_crystals.f90`.
// - The `.dat` outputs were generated locally from the bundled `Fingerprint2` source and frozen
//   into `tests/fixtures/fingerprint2/`.
// - These tests compare the full environment-fingerprint arrays for one cluster and one periodic
//   crystal fixture, plus the assignment distance reported by `fp_distance.f90`.

#[test]
fn benzene_cluster_environment_fingerprints_match_fingerprint2_fixture() {
    let structure = read_cluster_fixture(&fixture_path("benzene.xyz"));
    let expected = read_fingerprint_dat(&fixture_path("benzene_fingerprint.dat"));
    let engine = EnvironmentOverlapFingerprintEngine {
        config: EnvironmentOverlapFingerprintConfig {
            width_cutoff: FINGERPRINT2_WIDTH_CUTOFF_BOHR / ANGSTROM_TO_BOHR,
            ..EnvironmentOverlapFingerprintConfig::default()
        },
    };

    let actual = engine
        .fingerprint_environments(&structure)
        .expect("cluster fingerprints");

    assert_eq!(actual.environment_count, expected.len());
    assert_eq!(actual.fingerprint_length, expected[0].len());
    assert_matrix_close(
        &actual
            .environments
            .iter()
            .map(|env| env.values.clone())
            .collect::<Vec<_>>(),
        &expected,
        PARITY_TOLERANCE,
    );
}

#[test]
fn lial_hydrate_framework_environment_fingerprints_match_fingerprint2_fixture() {
    let structure = read_framework_ascii_fixture(&fixture_path("lial_hydrate.ascii"));
    let expected = read_fingerprint_dat(&fixture_path("lial_hydrate_fingerprint.dat"));
    let engine = EnvironmentOverlapFingerprintEngine {
        config: EnvironmentOverlapFingerprintConfig {
            width_cutoff: FINGERPRINT2_WIDTH_CUTOFF_BOHR / ANGSTROM_TO_BOHR,
            ..EnvironmentOverlapFingerprintConfig::default()
        },
    };

    let actual = engine
        .fingerprint_environments(&structure)
        .expect("framework fingerprints");

    assert_eq!(actual.environment_count, expected.len());
    assert_eq!(actual.fingerprint_length, expected[0].len());
    assert_matrix_close(
        &actual
            .environments
            .iter()
            .map(|env| env.values.clone())
            .collect::<Vec<_>>(),
        &expected,
        PARITY_TOLERANCE,
    );
}

#[test]
fn perovskite_assignment_distance_matches_fingerprint2_fixture() {
    let (left, right) = read_framework_ascii_fixture_pair(&fixture_path("perovskite.ascii"));
    let expected = read_scalar_fixture(&fixture_path("perovskite_assignment_distance.txt"));
    let engine = AssignmentFingerprintDistanceEngine {
        config: EnvironmentOverlapFingerprintConfig {
            width_cutoff: FINGERPRINT2_WIDTH_CUTOFF_BOHR / ANGSTROM_TO_BOHR,
            max_atoms_in_sphere: 100,
            s_orbital_count: 1,
            p_orbital_count: 1,
        },
    };

    let actual = engine
        .distance_frameworks(&left, &right)
        .expect("assignment distance");

    let delta = (actual.distance - expected).abs();
    assert!(
        delta <= DISTANCE_PARITY_TOLERANCE,
        "distance mismatch: actual={:.16e}, expected={:.16e}, delta={:.16e}, tolerance={:.16e}",
        actual.distance,
        expected,
        delta,
        DISTANCE_PARITY_TOLERANCE
    );
}

fn fixture_path(name: &str) -> String {
    format!("{FIXTURES_DIR}/{name}")
}

fn read_cluster_fixture(path: &str) -> Cluster0D {
    let frame = read_fingerprint2_cluster_xyz(Path::new(path));
    Cluster0D::new(
        StructureMetadata {
            label: "benzene".into(),
        },
        frame
            .atoms
            .into_iter()
            .map(|atom| Site {
                species: atom.species,
                coords: atom.coords,
            })
            .collect(),
        CoordinateBasis::Cartesian,
        None,
    )
    .expect("cluster")
}

fn read_fingerprint2_cluster_xyz(path: &Path) -> patina_sci_kernel::codec::xyz::XyzFrame {
    let text = fs::read_to_string(path).expect("cluster xyz");
    let mut lines = text.lines();
    let first = lines.next().expect("first line");
    let first_parts = first.split_whitespace().collect::<Vec<_>>();
    assert!(!first_parts.is_empty(), "invalid first line: {first}");
    let atom_count = first_parts[0].parse::<usize>().expect("atom count");
    let comment = if first_parts.len() > 1 {
        first_parts[1..].join(" ")
    } else {
        String::new()
    };
    let _coord_mode = lines.next().expect("coord mode line");
    let atoms = lines
        .take(atom_count)
        .map(|line| {
            let parts = line.split_whitespace().collect::<Vec<_>>();
            assert!(parts.len() >= 4, "invalid atom line: {line}");
            patina_sci_kernel::codec::xyz::AtomRecord {
                species: parts[0].to_string(),
                coords: [
                    parts[1].parse::<f64>().expect("x"),
                    parts[2].parse::<f64>().expect("y"),
                    parts[3].parse::<f64>().expect("z"),
                ],
            }
        })
        .collect::<Vec<_>>();
    patina_sci_kernel::codec::xyz::XyzFrame {
        atom_count,
        comment,
        atoms,
    }
}

fn read_framework_ascii_fixture(path: &str) -> Framework3D {
    let text = fs::read_to_string(path).expect("ascii fixture");
    let mut lines = text.lines();
    read_framework_ascii_configuration(&mut lines, "lial_hydrate")
}

fn read_framework_ascii_fixture_pair(path: &str) -> (Framework3D, Framework3D) {
    let text = fs::read_to_string(path).expect("ascii fixture");
    let mut lines = text.lines();
    let first = read_framework_ascii_configuration(&mut lines, "config_1");
    let second = read_framework_ascii_configuration(&mut lines, "config_2");
    (first, second)
}

fn read_framework_ascii_configuration<'a>(
    lines: &mut impl Iterator<Item = &'a str>,
    label: &str,
) -> Framework3D {
    let nat: usize = lines.next().expect("nat line").trim().parse().expect("nat");
    let line1 = parse_float_line(lines.next().expect("line 1"));
    let line2 = parse_float_line(lines.next().expect("line 2"));
    let lattice = [
        [line1[0], 0.0, 0.0],
        [line1[1], line1[2], 0.0],
        [line2[0], line2[1], line2[2]],
    ];
    let mut sites = Vec::with_capacity(nat);
    for _ in 0..nat {
        let line = lines.next().expect("atom line");
        let parts = line.split_whitespace().collect::<Vec<_>>();
        assert!(parts.len() >= 4, "invalid atom line: {line}");
        sites.push(Site {
            coords: [
                parts[0].parse::<f64>().expect("x"),
                parts[1].parse::<f64>().expect("y"),
                parts[2].parse::<f64>().expect("z"),
            ],
            species: parts[3].to_string(),
        });
    }
    Framework3D::new(
        StructureMetadata {
            label: label.into(),
        },
        sites,
        CoordinateBasis::Cartesian,
        Some(Lattice3::new(lattice)),
    )
    .expect("framework")
}

fn parse_float_line(line: &str) -> [f64; 3] {
    let parts = line.split_whitespace().collect::<Vec<_>>();
    assert!(parts.len() >= 3, "invalid float line: {line}");
    [
        parts[0].parse::<f64>().expect("f0"),
        parts[1].parse::<f64>().expect("f1"),
        parts[2].parse::<f64>().expect("f2"),
    ]
}

fn read_fingerprint_dat(path: &str) -> Vec<Vec<f64>> {
    let text = fs::read_to_string(path).expect("fingerprint dat");
    text.lines()
        .filter(|line| !line.trim().is_empty())
        .map(|line| {
            line.split_whitespace()
                .map(|value| value.parse::<f64>().expect("float"))
                .collect::<Vec<_>>()
        })
        .collect()
}

fn read_scalar_fixture(path: &str) -> f64 {
    fs::read_to_string(path)
        .expect("scalar fixture")
        .trim()
        .parse::<f64>()
        .expect("scalar value")
}

fn assert_matrix_close(actual: &[Vec<f64>], expected: &[Vec<f64>], tolerance: f64) {
    assert_eq!(actual.len(), expected.len(), "row count mismatch");
    for (row_index, (actual_row, expected_row)) in actual.iter().zip(expected.iter()).enumerate() {
        assert_eq!(
            actual_row.len(),
            expected_row.len(),
            "column count mismatch in row {row_index}"
        );
        for (col_index, (actual_value, expected_value)) in
            actual_row.iter().zip(expected_row.iter()).enumerate()
        {
            let delta = (actual_value - expected_value).abs();
            assert!(
                delta <= tolerance,
                "matrix mismatch at ({row_index},{col_index}): actual={actual_value:.16e}, expected={expected_value:.16e}, delta={delta:.16e}, tolerance={tolerance:.16e}"
            );
        }
    }
}
