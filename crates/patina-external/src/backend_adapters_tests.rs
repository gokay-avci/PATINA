use super::*;
use std::fs;
#[cfg(unix)]
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::time::Duration;
use tempfile::tempdir;

fn sample_candidate() -> patina_types::Candidate {
    patina_types::Candidate {
        species: vec!["Ce".into(), "O".into()],
        fractional_coords: vec![[0.0, 0.0, 0.0], [0.5, 0.5, 0.5]],
        lattice: Some([[5.4, 0.0, 0.0], [0.0, 5.4, 0.0], [0.0, 0.0, 5.4]]),
        periodic_axes: [true, true, true],
        label: "ceria".into(),
    }
}

#[test]
fn gin_writer_injects_fractional_coordinates_from_template() {
    let temp = tempdir().expect("tempdir");
    let template_path = temp.path().join("template.gin");
    fs::write(
        &template_path,
        "\
opti conp property full nosymm phon comp
# KLMC3-RS EXPECT_ATOMS: 2
species
Ce core 4.0
O  core -2.0
# === KLMC3-RS COORDINATE INJECTION POINT ===
",
    )
    .expect("write template");

    let writer = GinWriter::from_template_file(&template_path).expect("load template");
    let output_path = temp.path().join("candidate.gin");
    writer
        .write_candidate(&sample_candidate(), &output_path)
        .expect("write candidate");

    let rendered = fs::read_to_string(output_path).expect("read output");
    let expected = include_str!("../tests/fixtures/expected_candidate.gin");
    assert_eq!(rendered, expected);
}

#[test]
fn gin_writer_rejects_atom_count_mismatch() {
    let temp = tempdir().expect("tempdir");
    let template_path = temp.path().join("template.gin");
    fs::write(
        &template_path,
        "\
opti
# KLMC3-RS EXPECT_ATOMS: 3
# === KLMC3-RS COORDINATE INJECTION POINT ===
",
    )
    .expect("write template");

    let writer = GinWriter::from_template_file(&template_path).expect("load template");
    let err = writer
        .write_candidate(&sample_candidate(), temp.path().join("candidate.gin"))
        .expect_err("atom-count mismatch should fail");
    assert!(matches!(err, EvalError::TemplateInvalid { .. }));
}

#[test]
fn gin_writer_patches_native_cartesian_cluster_template() {
    let temp = tempdir().expect("tempdir");
    let template_path = temp.path().join("native_cluster.gin");
    fs::write(
        &template_path,
        "\
opti conv conj nosymm
cartesian
Mg core   0.000000   0.000000   0.000000   2.0 1.0 0.0
O  core   1.000000   1.000000   1.000000  -2.0 1.0 0.0
species
Mg core  2.00000
O  core -2.00000
buckingham
Mg core O core  1428.500 0.2945  0.00 0.0 12.0
",
    )
    .expect("write template");

    let writer = GinWriter::from_template_file(&template_path).expect("load template");
    let candidate = patina_types::Candidate {
        species: vec!["Mg".into(), "O".into()],
        fractional_coords: vec![[2.5, 3.5, 4.5], [5.5, 6.5, 7.5]],
        lattice: None,
        periodic_axes: [false, false, false],
        label: "native-cluster".into(),
    };
    let output_path = temp.path().join("candidate.gin");
    writer
        .write_candidate(&candidate, &output_path)
        .expect("write candidate");

    let rendered = fs::read_to_string(output_path).expect("read output");
    assert!(rendered.contains("Mg core   2.500000   3.500000   4.500000 2.0 1.0 0.0"));
    assert!(rendered.contains("O  core   5.500000   6.500000   7.500000 -2.0 1.0 0.0"));
    assert!(rendered.contains("species\nMg core  2.00000"));
}

#[test]
fn got_parser_reads_energy_convergence_and_fractional_coords() {
    let got = include_str!("../tests/fixtures/converged_sample.got");
    let parsed = GotParser::parse_str(got).expect("parse got");

    assert_eq!(parsed.energy, -1234.56789);
    assert!(parsed.converged);
    assert_eq!(parsed.relaxed_candidate.species, vec!["Ce", "O"]);
    assert_eq!(
        parsed.relaxed_candidate.fractional_coords,
        vec![[0.0, 0.0, 0.0], [0.5, 0.5, 0.5]]
    );
}

#[test]
fn got_parser_fails_closed_when_energy_line_is_missing() {
    let got = include_str!("../tests/fixtures/missing_energy_sample.got");
    let err = GotParser::parse_str(got).expect_err("missing energy should fail");
    assert!(matches!(err, EvalError::ParseFailed { .. }));
}

#[test]
fn got_parser_rejects_too_many_failed_attempts_banner() {
    let got = "\
Final energy = -139654.42925220 eV
**** Too many failed attempts to optimise ****
Final cartesian coordinates of atoms :
1 Mg c 0.0 0.0 0.0
2 O  c 1.0 1.0 1.0
";
    let err = GotParser::parse_str(got).expect_err("failed optimization should fail closed");
    assert!(matches!(err, EvalError::ParseFailed { .. }));
}

#[test]
fn got_parser_surfaces_explicit_gulp_error_before_missing_energy() {
    let got = "\
Some banner
ERROR : Fractional coordinates supplied for non 3-D case
Job Finished
";
    let err = GotParser::parse_str(got).expect_err("gulp error should fail closed");
    match err {
        EvalError::ParseFailed { line, reason } => {
            assert_eq!(line, 2);
            assert!(reason.contains("Fractional coordinates supplied for non 3-D case"));
        }
        other => panic!("unexpected error: {other:?}"),
    }
}

#[test]
fn got_parser_collapses_shell_rows_to_physical_sites() {
    let got = "\
Final energy = -382.19854024 eV
Optimisation achieved
Final cartesian coordinates of atoms :
--------------------------------------------------------------------------------
   No.  Atomic        x           y          z          Radius
        Label       (Angs)      (Angs)     (Angs)       (Angs)
--------------------------------------------------------------------------------
     1  Sr    c     0.171382    0.149487   -0.100709    0.000000
     2  Sr    c    -0.127063    2.302418    2.457153    0.000000
     3  O     c     2.518080    2.518080    2.765760    0.000000
     4  O     c     2.490770    0.014690   -0.102538    0.000000
     5  O     s     2.524950    2.524933    2.753898    0.000000
     6  O     s     2.488797    0.023315   -0.084681    0.000000
--------------------------------------------------------------------------------
Final Cartesian derivatives :
";
    let parsed = GotParser::parse_str(got).expect("parse shell-model got");

    assert_eq!(parsed.energy, -382.19854024);
    assert!(parsed.converged);
    assert_eq!(parsed.relaxed_candidate.species, vec!["Sr", "Sr", "O", "O"]);
    assert_eq!(
        parsed.relaxed_candidate.fractional_coords,
        vec![
            [0.171382, 0.149487, -0.100709],
            [-0.127063, 2.302418, 2.457153],
            [2.518080, 2.518080, 2.765760],
            [2.490770, 0.014690, -0.102538],
        ]
    );
}

#[test]
fn got_parser_stops_fractional_block_before_internal_derivatives() {
    let got = "\
Final energy = -12.50000000 eV
Optimisation achieved
Final fractional coordinates of atoms :
--------------------------------------------------------------------------------
   No.  Atomic       x           y          z
--------------------------------------------------------------------------------
     1  Mg     0.000000    0.000000    0.250000
     2  O      0.500000    0.500000    0.750000
Final internal derivatives :
--------------------------------------------------------------------------------
";
    let parsed = GotParser::parse_str(got).expect("parse got");

    assert_eq!(parsed.energy, -12.5);
    assert!(parsed.converged);
    assert_eq!(parsed.relaxed_candidate.species, vec!["Mg", "O"]);
    assert_eq!(
        parsed.relaxed_candidate.fractional_coords,
        vec![[0.0, 0.0, 0.25], [0.5, 0.5, 0.75]]
    );
}

#[test]
fn mock_backend_is_deterministic() {
    let backend = MockBackend;
    let candidate = sample_candidate();
    let first = backend
        .evaluate(&candidate, Path::new("."))
        .expect("first evaluation");
    let second = backend
        .evaluate(&candidate, Path::new("."))
        .expect("second evaluation");
    assert_eq!(first.energy, second.energy);
}

#[test]
#[cfg(unix)]
fn gulp_backend_can_use_scott_shaped_layout_and_names() {
    let temp = tempdir().expect("tempdir");
    let template_path = temp.path().join("Master.gin.in");
    fs::write(
        &template_path,
        "\
opti
species
Ce core 4.0
O  core -2.0
# KLMC3-RS EXPECT_ATOMS: 2
# === KLMC3-RS COORDINATE INJECTION POINT ===
",
    )
    .expect("write template");

    let atoms_in = temp.path().join("atoms.in");
    fs::write(&atoms_in, "2\n").expect("write atoms.in");

    let script_path = temp.path().join("fake_gulp.sh");
    fs::write(
        &script_path,
        "#!/bin/sh\nprintf 'Final energy = -1.23 eV\nOptimisation achieved\nFinal fractional coordinates of atoms\n 1 Ce 0.0 0.0 0.0\n 2 O 0.5 0.5 0.5\n' > gulp_klmc.gout\n",
    )
    .expect("write fake gulp");
    let mut perms = fs::metadata(&script_path).expect("metadata").permissions();
    perms.set_mode(0o755);
    fs::set_permissions(&script_path, perms).expect("chmod");

    let backend = GulpBackend::new(&template_path, &script_path, Some(Duration::from_secs(5)))
        .expect("backend")
        .with_io_names("gulp_klmc.gin", "gulp_klmc.gout")
        .with_scott_sidecars(&template_path, Some(&atoms_in));

    let sandbox = temp.path().join("sandbox");
    fs::create_dir(&sandbox).expect("sandbox");
    let result = backend
        .evaluate(&sample_candidate(), &sandbox)
        .expect("evaluate");

    assert_eq!(result.energy, -1.23);
    assert_eq!(result.relaxed_candidate.lattice, sample_candidate().lattice);
    assert_eq!(
        result.relaxed_candidate.periodic_axes,
        sample_candidate().periodic_axes
    );
    assert_eq!(result.relaxed_candidate.label, sample_candidate().label);
    assert!(sandbox.join("gulp_klmc.gin").exists());
    assert!(sandbox.join("gulp_klmc.gout").exists());
    assert!(sandbox.join("Master.gin").exists());
    assert!(sandbox.join("atoms.in").exists());
    assert!(sandbox.join("seed.xyz").exists());
}

#[test]
#[cfg(unix)]
fn standalone_gulp_backend_uses_stdin_stdout_contract() {
    let temp = tempdir().expect("tempdir");
    let template_path = temp.path().join("Master.gin.in");
    fs::write(
        &template_path,
        "\
opti
species
Ce core 4.0
O  core -2.0
# KLMC3-RS EXPECT_ATOMS: 2
# === KLMC3-RS COORDINATE INJECTION POINT ===
",
    )
    .expect("write template");

    let script_path = temp.path().join("gulp");
    fs::write(
        &script_path,
        "#!/bin/sh\ncat >/dev/null\nprintf 'Final energy = -1.23 eV\nOptimisation achieved\nFinal fractional coordinates of atoms\n 1 Ce 0.0 0.0 0.0\n 2 O 0.5 0.5 0.5\n'\n",
    )
    .expect("write fake gulp");
    let mut perms = fs::metadata(&script_path).expect("metadata").permissions();
    perms.set_mode(0o755);
    fs::set_permissions(&script_path, perms).expect("chmod");

    let backend = GulpBackend::new(&template_path, &script_path, Some(Duration::from_secs(5)))
        .expect("backend")
        .with_io_names("gulp_klmc.gin", "gulp_klmc.gout");

    let sandbox = temp.path().join("sandbox");
    fs::create_dir(&sandbox).expect("sandbox");
    let result = backend
        .evaluate(&sample_candidate(), &sandbox)
        .expect("evaluate");

    assert_eq!(result.energy, -1.23);
    assert_eq!(result.relaxed_candidate.lattice, sample_candidate().lattice);
    assert_eq!(
        result.relaxed_candidate.periodic_axes,
        sample_candidate().periodic_axes
    );
    assert_eq!(result.relaxed_candidate.label, sample_candidate().label);
    let rendered_input =
        fs::read_to_string(sandbox.join("gulp_klmc.gin")).expect("read generated input");
    assert!(rendered_input.contains("fractional"));
    assert!(sandbox.join("gulp_klmc.gout").exists());
}

#[test]
fn single_point_patch_rewrites_keyword_line_explicitly() {
    let temp = tempdir().expect("tempdir");
    let input = temp.path().join("candidate.gin");
    fs::write(
        &input,
        "\
opti conv conj nosymm
cartesian
Mg core 0.0 0.0 0.0 2.0 1.0 0.0
",
    )
    .expect("write input");

    patch_gin_for_single_point(&input).expect("patch single point");
    let rendered = fs::read_to_string(&input).expect("read patched input");
    let first_line = rendered.lines().next().expect("first line");
    assert_eq!(first_line, "single nosymm");
}

#[test]
fn xyz_writer_uses_lattice_for_periodic_candidates() {
    let temp = tempdir().expect("tempdir");
    let xyz_path = temp.path().join("candidate.extxyz");
    write_candidate_xyz(&sample_candidate(), &xyz_path).expect("write xyz");
    let xyz = fs::read_to_string(&xyz_path).expect("read xyz");
    assert!(xyz.contains("Lattice=\"5.4000000000 0.0000000000 0.0000000000 0.0000000000 5.4000000000 0.0000000000 0.0000000000 0.0000000000 5.4000000000\""));
    assert!(xyz.contains("pbc=\"T T T\""));
    assert!(xyz.contains("Ce 0.0000000000 0.0000000000 0.0000000000"));
    assert!(xyz.contains("O 2.7000000000 2.7000000000 2.7000000000"));
}

#[test]
#[cfg(unix)]
fn janus_backend_reads_normalized_adapter_response() {
    let temp = tempdir().expect("tempdir");
    let script_path = temp.path().join("fake_janus.py");
    fs::write(
        &script_path,
        "\
#!/usr/bin/env python3
import argparse
import json

parser = argparse.ArgumentParser()
parser.add_argument('--input', required=True)
parser.add_argument('--output', required=True)
parser.add_argument('--mode')
parser.add_argument('--arch')
parser.add_argument('--model')
parser.add_argument('--device')
parser.add_argument('--dtype')
parser.add_argument('--optimizer')
parser.add_argument('--fmax')
parser.add_argument('--steps')
args = parser.parse_args()

result = {
    'energy': -12.34,
    'forces': [[0.1, 0.0, 0.0], [0.0, 0.2, 0.0]],
    'species': ['Ce', 'O'],
    'coords': [[1.0, 2.0, 3.0], [4.0, 5.0, 6.0]],
    'converged': True,
}
with open(args.output, 'w', encoding='utf-8') as handle:
    json.dump(result, handle)
",
    )
    .expect("write fake adapter");
    let mut perms = fs::metadata(&script_path).expect("metadata").permissions();
    perms.set_mode(0o755);
    fs::set_permissions(&script_path, perms).expect("chmod");

    let backend = JanusMaceBackend::new(
        JanusMaceConfig {
            python_bin: PathBuf::from("python3"),
            adapter_script: script_path,
            ..JanusMaceConfig::default()
        },
        Some(Duration::from_secs(5)),
    );

    let sandbox = temp.path().join("sandbox");
    fs::create_dir(&sandbox).expect("sandbox");
    let result = backend
        .evaluate(&sample_candidate(), &sandbox)
        .expect("evaluate");

    assert_eq!(result.energy, -12.34);
    assert_eq!(result.forces.len(), 2);
    assert_eq!(
        result.relaxed_candidate.fractional_coords,
        vec![[1.0, 2.0, 3.0], [4.0, 5.0, 6.0]]
    );
    assert!(sandbox.join("candidate.extxyz").exists());
    assert!(sandbox.join("janus_result.json").exists());
}

#[test]
#[cfg(unix)]
fn janus_backend_accepts_nonperiodic_axes_without_lattice() {
    let temp = tempdir().expect("tempdir");
    let script_path = temp.path().join("fake_janus_cluster_axes.py");
    fs::write(
        &script_path,
        "\
#!/usr/bin/env python3
import argparse
import json

parser = argparse.ArgumentParser()
parser.add_argument('--input', required=True)
parser.add_argument('--output', required=True)
parser.add_argument('--mode')
parser.add_argument('--arch')
parser.add_argument('--model')
parser.add_argument('--device')
parser.add_argument('--dtype')
parser.add_argument('--optimizer')
parser.add_argument('--fmax')
parser.add_argument('--steps')
args = parser.parse_args()

result = {
    'energy': -12.34,
    'forces': [[0.1, 0.0, 0.0], [0.0, 0.2, 0.0]],
    'species': ['Ce', 'O'],
    'coords': [[1.0, 2.0, 3.0], [4.0, 5.0, 6.0]],
    'periodic_axes': [False, False, False],
    'converged': True,
}
with open(args.output, 'w', encoding='utf-8') as handle:
    json.dump(result, handle)
",
    )
    .expect("write fake adapter");
    let mut perms = fs::metadata(&script_path).expect("metadata").permissions();
    perms.set_mode(0o755);
    fs::set_permissions(&script_path, perms).expect("chmod");

    let backend = JanusMaceBackend::new(
        JanusMaceConfig {
            python_bin: PathBuf::from("python3"),
            adapter_script: script_path,
            ..JanusMaceConfig::default()
        },
        Some(Duration::from_secs(5)),
    );

    let sandbox = temp.path().join("sandbox");
    fs::create_dir(&sandbox).expect("sandbox");
    let result = backend
        .evaluate(&sample_candidate(), &sandbox)
        .expect("evaluate");

    assert_eq!(result.energy, -12.34);
    assert_eq!(result.relaxed_candidate.lattice, None);
    assert_eq!(
        result.relaxed_candidate.periodic_axes,
        [false, false, false]
    );
    assert_eq!(
        result.relaxed_candidate.fractional_coords,
        vec![[1.0, 2.0, 3.0], [4.0, 5.0, 6.0]]
    );
}

#[test]
#[cfg(unix)]
fn janus_backend_preserves_periodic_lattice_from_adapter_response() {
    let temp = tempdir().expect("tempdir");
    let script_path = temp.path().join("fake_janus_periodic.py");
    fs::write(
        &script_path,
        "\
#!/usr/bin/env python3
import argparse
import json

parser = argparse.ArgumentParser()
parser.add_argument('--input', required=True)
parser.add_argument('--output', required=True)
parser.add_argument('--mode')
parser.add_argument('--arch')
parser.add_argument('--model')
parser.add_argument('--device')
parser.add_argument('--dtype')
parser.add_argument('--optimizer')
parser.add_argument('--fmax')
parser.add_argument('--steps')
args = parser.parse_args()

result = {
    'energy': -9.87,
    'forces': [[0.0, 0.0, 0.0], [0.0, 0.0, 0.0]],
    'species': ['Ce', 'O'],
    'coords': [[0.0, 0.0, 0.0], [2.7, 2.7, 2.7]],
    'lattice': [[5.4, 0.0, 0.0], [0.0, 5.4, 0.0], [0.0, 0.0, 5.4]],
    'periodic_axes': [True, True, True],
    'converged': True,
}
with open(args.output, 'w', encoding='utf-8') as handle:
    json.dump(result, handle)
",
    )
    .expect("write fake adapter");
    let mut perms = fs::metadata(&script_path).expect("metadata").permissions();
    perms.set_mode(0o755);
    fs::set_permissions(&script_path, perms).expect("chmod");

    let backend = JanusMaceBackend::new(
        JanusMaceConfig {
            python_bin: PathBuf::from("python3"),
            adapter_script: script_path,
            ..JanusMaceConfig::default()
        },
        Some(Duration::from_secs(5)),
    );

    let sandbox = temp.path().join("sandbox");
    fs::create_dir(&sandbox).expect("sandbox");
    let result = backend
        .evaluate(&sample_candidate(), &sandbox)
        .expect("evaluate");

    assert_eq!(result.energy, -9.87);
    assert_eq!(
        result.relaxed_candidate.lattice,
        Some([[5.4, 0.0, 0.0], [0.0, 5.4, 0.0], [0.0, 0.0, 5.4]])
    );
    assert_eq!(result.relaxed_candidate.periodic_axes, [true, true, true]);
    assert_eq!(
        result.relaxed_candidate.fractional_coords,
        vec![[0.0, 0.0, 0.0], [0.5, 0.5, 0.5]]
    );
}

#[test]
#[cfg(unix)]
fn janus_backend_surfaces_non_convergence() {
    let temp = tempdir().expect("tempdir");
    let script_path = temp.path().join("fake_janus_fail.py");
    fs::write(
        &script_path,
        "\
#!/usr/bin/env python3
import argparse
import json

parser = argparse.ArgumentParser()
parser.add_argument('--input', required=True)
parser.add_argument('--output', required=True)
parser.add_argument('--mode')
parser.add_argument('--arch')
parser.add_argument('--model')
parser.add_argument('--device')
parser.add_argument('--dtype')
parser.add_argument('--optimizer')
parser.add_argument('--fmax')
parser.add_argument('--steps')
args = parser.parse_args()

result = {
    'energy': -1.0,
    'forces': [],
    'species': ['Ce', 'O'],
    'coords': [[0.0, 0.0, 0.0], [1.0, 1.0, 1.0]],
    'converged': False,
}
with open(args.output, 'w', encoding='utf-8') as handle:
    json.dump(result, handle)
",
    )
    .expect("write fake adapter");
    let mut perms = fs::metadata(&script_path).expect("metadata").permissions();
    perms.set_mode(0o755);
    fs::set_permissions(&script_path, perms).expect("chmod");

    let backend = JanusMaceBackend::new(
        JanusMaceConfig {
            python_bin: PathBuf::from("python3"),
            adapter_script: script_path,
            steps: 17,
            ..JanusMaceConfig::default()
        },
        Some(Duration::from_secs(5)),
    );

    let sandbox = temp.path().join("sandbox");
    fs::create_dir(&sandbox).expect("sandbox");
    let err = backend
        .evaluate(&sample_candidate(), &sandbox)
        .expect_err("non-converged result should fail");
    match err {
        EvalError::NotConverged {
            energy,
            n_steps,
            partial_result,
        } => {
            assert_eq!(energy, -1.0);
            assert_eq!(n_steps, 17);
            let partial = partial_result.expect("partial result");
            assert_eq!(partial.relaxed_candidate.label, "ceria");
            assert!(!partial.converged);
        }
        other => panic!("expected NotConverged, got {other:?}"),
    }
}

#[test]
#[cfg(unix)]
fn persistent_janus_backend_reuses_one_python_worker_per_thread() {
    let temp = tempdir().expect("tempdir");
    let script_path = temp.path().join("fake_janus_serve.py");
    let startup_count = temp.path().join("startup_count.txt");
    let request_count = temp.path().join("request_count.txt");
    fs::write(
        &script_path,
        format!(
            "\
#!/usr/bin/env python3
import json
import sys
from pathlib import Path

startup_path = Path({startup_path:?})
request_path = Path({request_path:?})

startup_total = 0
if startup_path.exists():
    startup_total = int(startup_path.read_text())
startup_path.write_text(str(startup_total + 1))
sys.stdout.write('{{\"status\":\"ready\"}}\\n')
sys.stdout.flush()

for raw in sys.stdin:
    request = json.loads(raw)
    if request.get('command') == 'shutdown':
        sys.stdout.write(json.dumps({{'status': 'ok', 'request_id': request.get('request_id')}}) + '\\n')
        sys.stdout.flush()
        break
    current = 0
    if request_path.exists():
        current = int(request_path.read_text())
    request_path.write_text(str(current + 1))
    result = {{
        'energy': -7.5,
        'forces': [[0.0, 0.0, 0.0], [0.0, 0.0, 0.0]],
        'species': ['Ce', 'O'],
        'coords': [[1.0, 2.0, 3.0], [4.0, 5.0, 6.0]],
        'converged': True,
    }}
    sys.stdout.write(json.dumps({{'status': 'ok', 'request_id': request.get('request_id'), 'result': result}}) + '\\n')
    sys.stdout.flush()
",
            startup_path = startup_count.display().to_string(),
            request_path = request_count.display().to_string(),
        ),
    )
    .expect("write fake adapter");
    let mut perms = fs::metadata(&script_path).expect("metadata").permissions();
    perms.set_mode(0o755);
    fs::set_permissions(&script_path, perms).expect("chmod");

    let backend = PersistentJanusMaceBackend::new(
        JanusMaceConfig {
            python_bin: PathBuf::from("python3"),
            adapter_script: script_path,
            ..JanusMaceConfig::default()
        },
        Some(Duration::from_secs(5)),
        temp.path().join("persistent-session"),
        false,
    )
    .expect("persistent backend");

    let sandbox_a = temp.path().join("sandbox-a");
    let sandbox_b = temp.path().join("sandbox-b");
    fs::create_dir(&sandbox_a).expect("sandbox a");
    fs::create_dir(&sandbox_b).expect("sandbox b");

    let first = backend
        .evaluate(&sample_candidate(), &sandbox_a)
        .expect("first evaluation");
    let second = backend
        .evaluate(&sample_candidate(), &sandbox_b)
        .expect("second evaluation");

    assert_eq!(first.energy, -7.5);
    assert_eq!(second.energy, -7.5);
    assert_eq!(
        fs::read_to_string(&startup_count).expect("startup count"),
        "1"
    );
    assert_eq!(
        fs::read_to_string(&request_count).expect("request count"),
        "2"
    );
    assert!(sandbox_a.join("janus_result.json").exists());
    assert!(sandbox_b.join("janus_result.json").exists());
}

#[test]
#[cfg(unix)]
fn persistent_janus_backend_waits_for_clean_shutdown_before_cleanup() {
    let temp = tempdir().expect("tempdir");
    let script_path = temp.path().join("fake_janus_shutdown.py");
    let shutdown_marker = temp.path().join("shutdown_complete.txt");
    let session_dir = temp.path().join("persistent-session");
    fs::write(
        &script_path,
        format!(
            "\
#!/usr/bin/env python3
import json
import sys
import time
from pathlib import Path

shutdown_marker = Path({shutdown_marker:?})
sys.stdout.write('{{\"status\":\"ready\"}}\\n')
sys.stdout.flush()

for raw in sys.stdin:
    request = json.loads(raw)
    if request.get('command') == 'shutdown':
        time.sleep(0.2)
        shutdown_marker.write_text('clean-exit')
        sys.stdout.write(json.dumps({{'status': 'ok', 'request_id': request.get('request_id')}}) + '\\n')
        sys.stdout.flush()
        break
    result = {{
        'energy': -7.5,
        'forces': [[0.0, 0.0, 0.0], [0.0, 0.0, 0.0]],
        'species': ['Ce', 'O'],
        'coords': [[1.0, 2.0, 3.0], [4.0, 5.0, 6.0]],
        'converged': True,
    }}
    sys.stdout.write(json.dumps({{'status': 'ok', 'request_id': request.get('request_id'), 'result': result}}) + '\\n')
    sys.stdout.flush()
",
            shutdown_marker = shutdown_marker.display().to_string(),
        ),
    )
    .expect("write fake adapter");
    let mut perms = fs::metadata(&script_path).expect("metadata").permissions();
    perms.set_mode(0o755);
    fs::set_permissions(&script_path, perms).expect("chmod");

    {
        let backend = PersistentJanusMaceBackend::new(
            JanusMaceConfig {
                python_bin: PathBuf::from("python3"),
                adapter_script: script_path,
                ..JanusMaceConfig::default()
            },
            Some(Duration::from_secs(5)),
            &session_dir,
            false,
        )
        .expect("persistent backend");

        let sandbox = temp.path().join("sandbox");
        fs::create_dir(&sandbox).expect("sandbox");
        backend
            .evaluate(&sample_candidate(), &sandbox)
            .expect("evaluate");
    }

    assert_eq!(
        fs::read_to_string(&shutdown_marker).expect("shutdown marker"),
        "clean-exit"
    );
    assert!(!session_dir.exists());
}

#[test]
#[cfg(unix)]
fn scott_backend_stages_expected_sandbox_layout() {
    let temp = tempdir().expect("tempdir");
    let template_path = temp.path().join("Master.gin.in");
    fs::write(
        &template_path,
        "\
opti
species
Ce core 4.0
O  core -2.0
# KLMC3-RS EXPECT_ATOMS: 2
# === KLMC3-RS COORDINATE INJECTION POINT ===
",
    )
    .expect("write template");

    let run_job = temp.path().join("run.job");
    fs::write(&run_job, "JOB_TYPE:2\n").expect("write run.job");
    let atoms_in = temp.path().join("atoms.in");
    fs::write(&atoms_in, "2\n").expect("write atoms.in");

    let script_path = temp.path().join("klmc_scott.sh");
    fs::write(
        &script_path,
        "#!/bin/sh\nmkdir -p run/0\nprintf 'Final energy = -1.23 eV\nOptimisation achieved\nFinal fractional coordinates of atoms\n 1 Ce 0.0 0.0 0.0\n 2 O 0.5 0.5 0.5\n' > run/0/gulp_klmc.gout\n",
    )
    .expect("write fake scott");
    let mut perms = fs::metadata(&script_path).expect("metadata").permissions();
    perms.set_mode(0o755);
    fs::set_permissions(&script_path, perms).expect("chmod");

    let backend = ScottBackend::new(
        &template_path,
        &script_path,
        ScottSandboxTemplate::new(&run_job, &atoms_in, None::<&Path>),
        Some(Duration::from_secs(5)),
    )
    .expect("backend");

    let sandbox = temp.path().join("sandbox");
    fs::create_dir(&sandbox).expect("sandbox");
    let result = backend
        .evaluate(&sample_candidate(), &sandbox)
        .expect("evaluate");

    assert_eq!(result.energy, -1.23);
    assert!(sandbox.join("data/jobs").exists());
    assert!(sandbox.join("data/run.job").exists());
    assert!(sandbox.join("data/atoms.in").exists());
    assert!(sandbox.join("data/Master.gin").exists());
    assert!(sandbox.join("run/0/gulp_klmc.gout").exists());
}
