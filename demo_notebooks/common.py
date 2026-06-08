from __future__ import annotations

import json
import shutil
import subprocess
from dataclasses import dataclass
from pathlib import Path
from typing import Any

import pandas as pd
import plotly.express as px

REPO_ROOT = Path(__file__).resolve().parents[1]
DEMO_ROOT = REPO_ROOT / "demo_notebooks"
INPUTS_DIR = DEMO_ROOT / "inputs"
RUNS_DIR = DEMO_ROOT / "runs"


@dataclass
class CommandResult:
    args: list[str]
    stdout: str
    stderr: str
    returncode: int

    def json(self) -> Any:
        return json.loads(self.stdout)


def ensure_runs_dir() -> Path:
    RUNS_DIR.mkdir(parents=True, exist_ok=True)
    return RUNS_DIR


def build_driver() -> CommandResult:
    return run_command(["cargo", "build", "-p", "patina-driver"])


def driver_executable() -> Path:
    return REPO_ROOT / "target" / "debug" / "patina-driver"


def driver_prefix() -> list[str]:
    exe = driver_executable()
    if exe.exists():
        return [str(exe)]
    return ["cargo", "run", "-q", "-p", "patina-driver", "--"]


def run_command(args: list[str], cwd: Path | None = None, check: bool = True) -> CommandResult:
    completed = subprocess.run(
        args,
        cwd=cwd or REPO_ROOT,
        text=True,
        capture_output=True,
        check=False,
    )
    result = CommandResult(
        args=args,
        stdout=completed.stdout.strip(),
        stderr=completed.stderr.strip(),
        returncode=completed.returncode,
    )
    if check and completed.returncode != 0:
        raise RuntimeError(
            f"command failed ({completed.returncode}): {' '.join(args)}\n\n"
            f"stdout:\n{result.stdout}\n\nstderr:\n{result.stderr}"
        )
    return result


def run_driver(*args: str, check: bool = True) -> CommandResult:
    return run_command([*driver_prefix(), *args], check=check)


def workflow_text(*args: str) -> str:
    return run_driver("workflow", *args).stdout


def workflow_json(*args: str) -> Any:
    return run_driver("workflow", *args).json()


def shell_command(*args: str) -> str:
    return " ".join(driver_prefix() + list(args))


def read_json(path: Path) -> Any:
    return json.loads(path.read_text())


def write_json(path: Path, payload: Any) -> Path:
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(json.dumps(payload, indent=2))
    return path


def write_text(path: Path, text: str) -> Path:
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(text)
    return path


def reset_demo_run(name: str) -> Path:
    ensure_runs_dir()
    run_dir = RUNS_DIR / name
    if run_dir.exists():
        shutil.rmtree(run_dir)
    run_dir.mkdir(parents=True, exist_ok=True)
    return run_dir


def cluster_input_path() -> Path:
    return INPUTS_DIR / "cluster_seed.json"


def framework_input_path() -> Path:
    return INPUTS_DIR / "framework_parent.json"


def load_cluster_candidate() -> dict[str, Any]:
    return read_json(cluster_input_path())


def load_framework_candidate() -> dict[str, Any]:
    return read_json(framework_input_path())


def candidate_frame(candidate: dict[str, Any]) -> pd.DataFrame:
    return pd.DataFrame(
        {
            "site": list(range(len(candidate["species"]))),
            "species": candidate["species"],
            "x": [row[0] for row in candidate["fractional_coords"]],
            "y": [row[1] for row in candidate["fractional_coords"]],
            "z": [row[2] for row in candidate["fractional_coords"]],
        }
    )


def plot_candidate(candidate: dict[str, Any], title: str | None = None):
    frame = candidate_frame(candidate)
    fig = px.scatter_3d(
        frame,
        x="x",
        y="y",
        z="z",
        color="species",
        text="site",
        title=title or candidate.get("label", "candidate"),
    )
    fig.update_traces(marker={"size": 7}, textposition="top center")
    fig.update_layout(scene_aspectmode="data")
    return fig


def pretty_json(value: Any) -> str:
    return json.dumps(value, indent=2)


def manifest_summary_frame(manifest: dict[str, Any]) -> pd.DataFrame:
    summary = manifest.get("summary", {})
    return pd.DataFrame(
        [{"field": key, "value": json.dumps(value) if isinstance(value, (dict, list)) else value} for key, value in summary.items()]
    )


def provenance_frame(manifest: dict[str, Any]) -> pd.DataFrame:
    provenance = manifest.get("provenance", {})
    return pd.DataFrame(
        [{"field": key, "value": json.dumps(value) if isinstance(value, (dict, list)) else value} for key, value in provenance.items()]
    )


def artifact_frame(manifest: dict[str, Any], run_dir: Path) -> pd.DataFrame:
    artifacts = manifest.get("artifacts", {})
    return pd.DataFrame(
        [
            {
                "artifact": key,
                "relative_path": value,
                "exists": (run_dir / value).exists(),
            }
            for key, value in artifacts.items()
        ]
    )


def cluster_perturbation_spec_text(run_dir: Path, candidate_json: Path) -> str:
    return f"""workflow = "structure.perturb-cluster"

[run]
run_dir = "{run_dir.as_posix()}"
system = "demo-cluster"

[seed]
candidate_json = "{candidate_json.as_posix()}"

[perturbation]
count = 6
sigma = 0.12
max_displacement = 0.2
duplicate_threshold = 0.000001
duplicate_screening_mode = "global-overlap"
include_p_orbitals = false
environment_width_cutoff = 1.0
environment_max_atoms_in_sphere = 8
environment_s_orbital_count = 1
environment_p_orbital_count = 0
seed = 7
"""


def surface_generation_spec_text(run_dir: Path, candidate_json: Path) -> str:
    return f"""workflow = "framework.generate-surface"

[run]
run_dir = "{run_dir.as_posix()}"
system = "demo-surface"

[input]
candidate_json = "{candidate_json.as_posix()}"

[surface]
h = 1
k = 0
l = 0
thickness = 8.0
vacuum = 10.0
supercell_a = 1
supercell_b = 1
cut_strategy = "topology-aware"
dedup_slab = true
reduce_slab_inplane = true
dedup_frac_tol = 0.0001
dedup_wrap_z = false
dedup_ignore_element = false
"""
