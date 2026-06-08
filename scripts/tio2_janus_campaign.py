#!/usr/bin/env python3
from __future__ import annotations

import argparse
import json
import math
import shutil
import subprocess
import sys
from dataclasses import dataclass
from pathlib import Path
from typing import Iterable


PROJECT_ROOT = Path(__file__).resolve().parent.parent
CAMPAIGN_ROOT = PROJECT_ROOT / "experiments" / "tio2_janus_persistent"
INPUTS_DIR = CAMPAIGN_ROOT / "inputs"
SPECS_DIR = CAMPAIGN_ROOT / "specs"
LOGS_DIR = CAMPAIGN_ROOT / "logs"
ATOMS_IN_PATH = INPUTS_DIR / "atoms.tio2.in"
RUNS_ROOT = PROJECT_ROOT / "runs" / "active" / "tio2_janus_persistent"
WORK_ROOT = PROJECT_ROOT / "scratch" / "tio2_janus_persistent"
RESUME_SPECS_DIR = WORK_ROOT / "_resume_specs"
JANUS_PYTHON = PROJECT_ROOT / "venvs" / "janus" / "bin" / "python"
JANUS_ADAPTER = (
    PROJECT_ROOT / "crates" / "patina-external" / "python" / "janus_mace_adapter.py"
)


@dataclass(frozen=True)
class CampaignSettings:
    start: int
    end: int
    generations: int
    population: int
    workers: int
    temperature: float
    step_size: float
    seed_base: int
    janus_model: str
    janus_model_path: Path | None
    janus_arch: str
    janus_device: str
    janus_dtype: str
    janus_mode: str
    janus_optimizer: str
    janus_fmax: float
    janus_steps: int
    keep_dirs: bool
    timeout_secs: int | None
    use_dreadnaut_keys: bool
    hashkey_radius: str
    hashkey_radius_const: float
    pmoi_tolerance: float
    enable_pmoi: bool
    python_bin: Path
    janus_adapter_script: Path


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(
        description="Prepare and run TiO2 Janus persistent-daemon GA campaigns."
    )
    subparsers = parser.add_subparsers(dest="command", required=True)

    write_inputs = subparsers.add_parser(
        "write-inputs",
        help="Generate TiO2 seed XYZ files and workflow specs.",
    )
    add_common_range_args(write_inputs)
    add_common_campaign_args(write_inputs)

    validate = subparsers.add_parser(
        "validate",
        help="Generate TiO2 specs and validate them through patina-driver.",
    )
    add_common_range_args(validate)
    add_common_campaign_args(validate)

    status = subparsers.add_parser(
        "status",
        help="Report campaign state for each TiO2 cluster size.",
    )
    add_common_range_args(status)
    add_common_campaign_args(status)

    run = subparsers.add_parser(
        "run",
        help="Run or resume the TiO2 Janus persistent-daemon campaign.",
    )
    add_common_range_args(run)
    add_common_campaign_args(run)
    run.add_argument(
        "--dry-run",
        action="store_true",
        help="Print commands without executing them.",
    )
    run.add_argument(
        "--continue-on-error",
        action="store_true",
        help="Continue with later n values when one run fails.",
    )
    run.add_argument(
        "--reset-stale",
        action="store_true",
        help="Delete run/work directories for entries with a manifest but no checkpoint, then restart them.",
    )

    return parser.parse_args()


def add_common_range_args(parser: argparse.ArgumentParser) -> None:
    parser.add_argument("--start", type=int, default=1, help="First TiO2 cluster size n.")
    parser.add_argument("--end", type=int, default=40, help="Last TiO2 cluster size n.")


def add_common_campaign_args(parser: argparse.ArgumentParser) -> None:
    parser.add_argument("--generations", type=int, default=30)
    parser.add_argument("--population", type=int, default=24)
    parser.add_argument("--workers", type=int, default=4)
    parser.add_argument("--temperature", type=float, default=10.0)
    parser.add_argument("--step-size", type=float, default=0.18)
    parser.add_argument("--seed-base", type=int, default=4100)
    parser.add_argument("--janus-model", default="small")
    parser.add_argument(
        "--janus-model-path",
        type=Path,
        default=None,
        help="Local Janus/MACE .model file. Overrides --janus-model when set.",
    )
    parser.add_argument("--janus-arch", default="mace_mp")
    parser.add_argument("--janus-device", default="cpu")
    parser.add_argument("--janus-dtype", default="float64")
    parser.add_argument("--janus-mode", choices=["single-point", "local-opt"], default="local-opt")
    parser.add_argument(
        "--janus-optimizer",
        choices=["lbfgs", "fire", "fire2", "abc-fire"],
        default="abc-fire",
    )
    parser.add_argument("--janus-fmax", type=float, default=0.1)
    parser.add_argument("--janus-steps", type=int, default=1000)
    parser.add_argument("--keep-dirs", action="store_true")
    parser.add_argument("--timeout-secs", type=int, default=None)
    parser.add_argument("--no-dreadnaut-keys", action="store_true")
    parser.add_argument("--hashkey-radius", default="IR")
    parser.add_argument("--hashkey-radius-const", type=float, default=0.4)
    parser.add_argument("--pmoi-tolerance", type=float, default=0.02)
    parser.add_argument("--disable-pmoi", action="store_true")
    parser.add_argument("--python-bin", type=Path, default=None)
    parser.add_argument("--janus-adapter-script", type=Path, default=None)


def build_settings(args: argparse.Namespace) -> CampaignSettings:
    if args.start < 1 or args.end < args.start:
        raise SystemExit("--start must be >= 1 and --end must be >= --start")
    if args.generations < 1:
        raise SystemExit("--generations must be at least 1")
    if args.population < 2:
        raise SystemExit("--population must be at least 2")
    if args.workers < 1:
        raise SystemExit("--workers must be at least 1")

    return CampaignSettings(
        start=args.start,
        end=args.end,
        generations=args.generations,
        population=args.population,
        workers=args.workers,
        temperature=args.temperature,
        step_size=args.step_size,
        seed_base=args.seed_base,
        janus_model=args.janus_model,
        janus_model_path=absolutize(args.janus_model_path) if args.janus_model_path else None,
        janus_arch=args.janus_arch,
        janus_device=args.janus_device,
        janus_dtype=args.janus_dtype,
        janus_mode=args.janus_mode,
        janus_optimizer=args.janus_optimizer,
        janus_fmax=args.janus_fmax,
        janus_steps=args.janus_steps,
        keep_dirs=args.keep_dirs,
        timeout_secs=args.timeout_secs,
        use_dreadnaut_keys=not args.no_dreadnaut_keys,
        hashkey_radius=args.hashkey_radius,
        hashkey_radius_const=args.hashkey_radius_const,
        pmoi_tolerance=args.pmoi_tolerance,
        enable_pmoi=not args.disable_pmoi,
        python_bin=absolutize(args.python_bin or JANUS_PYTHON),
        janus_adapter_script=absolutize(args.janus_adapter_script or JANUS_ADAPTER),
    )


def iter_ns(settings: CampaignSettings) -> Iterable[int]:
    return range(settings.start, settings.end + 1)


def tag_for_n(n: int) -> str:
    return f"tio2_n{n:02d}"


def system_label(n: int) -> str:
    return f"(TiO2){n}"


def seed_path(n: int) -> Path:
    return INPUTS_DIR / f"{tag_for_n(n)}.xyz"


def spec_path(n: int) -> Path:
    return SPECS_DIR / f"{tag_for_n(n)}.toml"


def run_dir(n: int) -> Path:
    return RUNS_ROOT / tag_for_n(n)


def work_dir(n: int) -> Path:
    return WORK_ROOT / tag_for_n(n)


def checkpoint_path(n: int) -> Path:
    return run_dir(n) / "raw" / "rust_ga_checkpoint_latest.json"


def manifest_path(n: int) -> Path:
    return run_dir(n) / "manifest.json"


def absolutize(path: Path) -> Path:
    expanded = path.expanduser()
    if expanded.is_absolute():
        return expanded
    return PROJECT_ROOT / expanded


def ensure_campaign_dirs() -> None:
    for path in [CAMPAIGN_ROOT, INPUTS_DIR, SPECS_DIR, LOGS_DIR, RUNS_ROOT, WORK_ROOT, RESUME_SPECS_DIR]:
        path.mkdir(parents=True, exist_ok=True)


def effective_janus_model(settings: CampaignSettings) -> str:
    if settings.janus_model_path is not None:
        return str(settings.janus_model_path)
    return settings.janus_model


def normalize(vector: tuple[float, float, float]) -> tuple[float, float, float]:
    x, y, z = vector
    norm = math.sqrt(x * x + y * y + z * z)
    if norm == 0.0:
        return (0.0, 0.0, 1.0)
    return (x / norm, y / norm, z / norm)


def cross(
    left: tuple[float, float, float], right: tuple[float, float, float]
) -> tuple[float, float, float]:
    lx, ly, lz = left
    rx, ry, rz = right
    return (ly * rz - lz * ry, lz * rx - lx * rz, lx * ry - ly * rx)


def add(
    left: tuple[float, float, float], right: tuple[float, float, float]
) -> tuple[float, float, float]:
    return (left[0] + right[0], left[1] + right[1], left[2] + right[2])


def scale(vector: tuple[float, float, float], factor: float) -> tuple[float, float, float]:
    return (vector[0] * factor, vector[1] * factor, vector[2] * factor)


def generate_ti_positions(n: int) -> list[tuple[float, float, float]]:
    if n == 1:
        return [(0.0, 0.0, 0.0)]

    golden_angle = math.pi * (3.0 - math.sqrt(5.0))
    radius = 0.95 + 0.40 * (n ** (1.0 / 3.0))
    positions: list[tuple[float, float, float]] = []
    for i in range(n):
        y = 1.0 - 2.0 * (i + 0.5) / n
        radial = math.sqrt(max(0.0, 1.0 - y * y))
        theta = golden_angle * i
        x = radius * radial * math.cos(theta)
        z = radius * radial * math.sin(theta)
        positions.append((x, radius * y, z))
    return positions


def generate_seed_cluster(n: int) -> list[tuple[str, float, float, float]]:
    if n == 1:
        return [
            ("Ti", 0.0, 0.0, 0.0),
            ("O", 0.0, 0.0, 1.62),
            ("O", 0.0, 0.0, -1.62),
        ]

    atoms: list[tuple[str, float, float, float]] = []
    ti_positions = generate_ti_positions(n)
    bond_length = 1.82
    z_axis = (0.0, 0.0, 1.0)
    x_axis = (1.0, 0.0, 0.0)

    for index, ti in enumerate(ti_positions):
        atoms.append(("Ti", ti[0], ti[1], ti[2]))
        radial = normalize(ti)
        reference = x_axis if abs(radial[2]) > 0.8 else z_axis
        tangent_a = normalize(cross(radial, reference))
        tangent_b = normalize(cross(radial, tangent_a))
        angle = 2.0 * math.pi * index / n
        tangent = normalize(
            add(scale(tangent_a, math.cos(angle)), scale(tangent_b, math.sin(angle)))
        )
        direction_a = normalize(add(radial, scale(tangent, 0.55)))
        direction_b = normalize(add(radial, scale(tangent, -0.55)))
        oxygen_a = add(ti, scale(direction_a, bond_length))
        oxygen_b = add(ti, scale(direction_b, bond_length))
        atoms.append(("O", oxygen_a[0], oxygen_a[1], oxygen_a[2]))
        atoms.append(("O", oxygen_b[0], oxygen_b[1], oxygen_b[2]))

    return recenter_atoms(atoms)


def recenter_atoms(
    atoms: list[tuple[str, float, float, float]]
) -> list[tuple[str, float, float, float]]:
    count = len(atoms)
    cx = sum(row[1] for row in atoms) / count
    cy = sum(row[2] for row in atoms) / count
    cz = sum(row[3] for row in atoms) / count
    return [(species, x - cx, y - cy, z - cz) for species, x, y, z in atoms]


def write_xyz(path: Path, atoms: list[tuple[str, float, float, float]], label: str) -> None:
    lines = [str(len(atoms)), label]
    for species, x, y, z in atoms:
        lines.append(f"{species} {x:.10f} {y:.10f} {z:.10f}")
    path.write_text("\n".join(lines) + "\n", encoding="utf-8")


def write_atoms_in(path: Path) -> None:
    lines = [
        "index,species,atomic_number,covalent_radius,ionic_radius",
        "1,Ti,22,1.60,0.61",
        "2,O,8,0.66,1.40",
    ]
    path.write_text("\n".join(lines) + "\n", encoding="utf-8")


def render_spec(
    settings: CampaignSettings,
    n: int,
    *,
    base_candidate_path: Path | None,
    resume_from_checkpoint: Path | None,
) -> str:
    timeout_line = (
        f"timeout_secs = {settings.timeout_secs}\n" if settings.timeout_secs is not None else ""
    )
    base_candidate_line = (
        f'base_candidate_path = "{base_candidate_path.resolve()}"\n'
        if base_candidate_path is not None
        else ""
    )
    resume_line = (
        f'resume_from_checkpoint = "{resume_from_checkpoint.resolve()}"\n'
        if resume_from_checkpoint is not None
        else ""
    )

    return (
        'workflow = "ga.persistent-daemon"\n\n'
        "[run]\n"
        f'run_dir = "{run_dir(n).resolve()}"\n'
        f'workdir = "{work_dir(n).resolve()}"\n'
        f'system = "{system_label(n)}"\n\n'
        "[seed]\n"
        f"{base_candidate_line}"
        f"{resume_line}"
        "\n"
        "[ga]\n"
        f"generations = {settings.generations}\n"
        f"population = {settings.population}\n"
        f"keep_dirs = {str(settings.keep_dirs).lower()}\n"
        f"seed = {settings.seed_base + n}\n"
        f"temperature = {settings.temperature}\n"
        f"step_size = {settings.step_size}\n\n"
        "[backend]\n"
        f"workers = {settings.workers}\n"
        f"keep_dirs = {str(settings.keep_dirs).lower()}\n"
        f"{timeout_line}"
        "\n"
        "[janus]\n"
        f'python_bin = "{settings.python_bin}"\n'
        f'janus_adapter_script = "{settings.janus_adapter_script}"\n'
        f'arch = "{settings.janus_arch}"\n'
        f'model = "{effective_janus_model(settings)}"\n'
        f'device = "{settings.janus_device}"\n'
        f'dtype = "{settings.janus_dtype}"\n'
        f'mode = "{settings.janus_mode}"\n'
        f'optimizer = "{settings.janus_optimizer}"\n'
        f"fmax = {settings.janus_fmax}\n"
        f"steps = {settings.janus_steps}\n\n"
        "[duplicate_policy]\n"
        'mode = "external-native-hashkey"\n'
        f'atoms_in_template = "{ATOMS_IN_PATH.resolve()}"\n'
        f"use_dreadnaut_keys = {str(settings.use_dreadnaut_keys).lower()}\n"
        f'hashkey_radius = "{settings.hashkey_radius}"\n'
        f"hashkey_radius_const = {settings.hashkey_radius_const}\n"
        f"pmoi_tolerance = {settings.pmoi_tolerance}\n"
        f"enable_pmoi = {str(settings.enable_pmoi).lower()}\n\n"
        "[operator_policy]\n"
    )


def write_fresh_inputs_and_specs(settings: CampaignSettings) -> None:
    ensure_campaign_dirs()
    write_atoms_in(ATOMS_IN_PATH)
    for n in iter_ns(settings):
        atoms = generate_seed_cluster(n)
        xyz_path = seed_path(n)
        write_xyz(xyz_path, atoms, system_label(n))
        toml = render_spec(
            settings,
            n,
            base_candidate_path=xyz_path,
            resume_from_checkpoint=None,
        )
        spec_path(n).write_text(toml, encoding="utf-8")


def write_resume_spec(settings: CampaignSettings, n: int, checkpoint: Path) -> Path:
    RESUME_SPECS_DIR.mkdir(parents=True, exist_ok=True)
    path = RESUME_SPECS_DIR / f"{tag_for_n(n)}.resume.toml"
    path.write_text(
        render_spec(
            settings,
            n,
            base_candidate_path=None,
            resume_from_checkpoint=checkpoint,
        ),
        encoding="utf-8",
    )
    return path


def load_json(path: Path) -> dict[str, object]:
    return json.loads(path.read_text(encoding="utf-8"))


def campaign_state(settings: CampaignSettings, n: int) -> tuple[str, Path]:
    checkpoint = checkpoint_path(n)
    manifest = manifest_path(n)
    if checkpoint.exists():
        checkpoint_json = load_json(checkpoint)
        completed = int(checkpoint_json.get("generation_completed", 0))
        if completed >= settings.generations:
            return ("completed", checkpoint)
        return (f"resume_from_generation_{completed}", checkpoint)
    if manifest.exists():
        return ("manifest_without_checkpoint", manifest)
    return ("fresh", spec_path(n))


def run_command(command: list[str], *, dry_run: bool) -> int:
    printable = " ".join(shell_quote(part) for part in command)
    print(printable)
    if dry_run:
        return 0
    return subprocess.run(command, check=False, cwd=PROJECT_ROOT).returncode


def shell_quote(value: str) -> str:
    if not value or any(ch in value for ch in " \t\n\"'()[]{}$&;<>|"):
        escaped = value.replace("'", "'\"'\"'")
        return f"'{escaped}'"
    return value


def validate_specs(settings: CampaignSettings) -> int:
    write_fresh_inputs_and_specs(settings)
    exit_code = 0
    for n in iter_ns(settings):
        command = [
            "cargo",
            "run",
            "-p",
            "patina-driver",
            "--",
            "workflow",
            "validate-spec",
            str(spec_path(n).resolve()),
        ]
        result = run_command(command, dry_run=False)
        if result != 0:
            exit_code = result
    return exit_code


def print_status(settings: CampaignSettings) -> int:
    write_fresh_inputs_and_specs(settings)
    print("n\tlabel\tstate\treference")
    for n in iter_ns(settings):
        state, reference = campaign_state(settings, n)
        print(f"{n}\t{tag_for_n(n)}\t{state}\t{reference}")
    return 0


def run_campaign(
    settings: CampaignSettings,
    *,
    dry_run: bool,
    continue_on_error: bool,
    reset_stale: bool,
) -> int:
    write_fresh_inputs_and_specs(settings)
    exit_code = 0

    for n in iter_ns(settings):
        state, reference = campaign_state(settings, n)
        if state == "completed":
            print(f"skip {tag_for_n(n)}: already completed up to generation {settings.generations}")
            continue
        if state == "manifest_without_checkpoint":
            if not reset_stale:
                print(
                    f"skip {tag_for_n(n)}: manifest exists without checkpoint; rerun with --reset-stale or inspect {reference}",
                    file=sys.stderr,
                )
                exit_code = 1
                if not continue_on_error:
                    return exit_code
                continue
            print(f"reset stale {tag_for_n(n)}: removing {run_dir(n)} and {work_dir(n)}")
            if not dry_run:
                shutil.rmtree(run_dir(n), ignore_errors=True)
                shutil.rmtree(work_dir(n), ignore_errors=True)
            state = "fresh"
            reference = spec_path(n)

        effective_spec = (
            write_resume_spec(settings, n, reference)
            if state.startswith("resume_from_generation_")
            else spec_path(n)
        )
        command = [
            "cargo",
            "run",
            "-p",
            "patina-driver",
            "--",
            "workflow",
            "run",
            str(effective_spec.resolve()),
        ]
        result = run_command(command, dry_run=dry_run)
        if result != 0:
            exit_code = result
            if not continue_on_error:
                return exit_code

    return exit_code


def main() -> int:
    args = parse_args()
    settings = build_settings(args)

    if args.command == "write-inputs":
        write_fresh_inputs_and_specs(settings)
        print(f"wrote inputs under {INPUTS_DIR}")
        print(f"wrote specs under {SPECS_DIR}")
        return 0
    if args.command == "validate":
        return validate_specs(settings)
    if args.command == "status":
        return print_status(settings)
    if args.command == "run":
        return run_campaign(
            settings,
            dry_run=args.dry_run,
            continue_on_error=args.continue_on_error,
            reset_stale=args.reset_stale,
        )
    raise SystemExit(f"unsupported command: {args.command}")


if __name__ == "__main__":
    raise SystemExit(main())
