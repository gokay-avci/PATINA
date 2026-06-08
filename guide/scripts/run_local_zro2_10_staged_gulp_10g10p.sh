#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "${SCRIPT_DIR}/../.." && pwd)"
STAMP="$(date +%Y%m%d_%H%M%S)"

GENERATIONS="${GENERATIONS:-50}"
POPULATION="${POPULATION:-50}"
SEED="${SEED:-240510}"
STEP_SIZE="${STEP_SIZE:-0.26}"
TEMPERATURE="${TEMPERATURE:-10.0}"

# Literature-oriented rigid-ion settings for zirconia in the current staged PATINA/GULP lane.
# The short-range Buckingham terms are the commonly reused ZrO2 values tabulated by
# Woodley, Battle, Gale and Catlow (PCCP 1999, doi:10.1039/A901227C) and reused in later
# zirconia GULP studies such as Dawson and Tanaka (J. Mater. Chem. A 2014,
# doi:10.1039/C3TA14029F). The target cluster size also sits inside the n = 1..12 zirconia
# nanocluster range explored by Woodley, Hamad and Catlow (PCCP 2010, doi:10.1039/C0CP00057D).
#
# PATINA's staged cluster templates currently operate as rigid-ion coordinate blocks, so this
# example uses formal charges with Buckingham pair terms rather than an oxygen shell model.
SCOTT_GNORM="${SCOTT_GNORM:-0.10}"
GULP_GTOL="${GULP_GTOL:-0.08}"
GULP_GMAX="${GULP_GMAX:-0.12}"
GULP_FTOL="${GULP_FTOL:-0.001}"
GULP_XTOL="${GULP_XTOL:-0.001}"
GULP_MAXCYC="${GULP_MAXCYC:-400}"
GULP_MINIMIZER="${GULP_MINIMIZER:-lbfgs}"

SYSTEM_LABEL="${SYSTEM_LABEL:-'(ZrO2)10'}"
RUN_NAME="${RUN_NAME:-zro2_10_staged_gulp_${GENERATIONS}g${POPULATION}p_${STAMP}}"
RUN_DIR="${RUN_DIR:-${REPO_ROOT}/runs/active/${RUN_NAME}}"
INPUT_DIR="${RUN_DIR}/inputs"
WORKDIR="${RUN_DIR}/workdir"
BASE_CANDIDATE_JSON="${INPUT_DIR}/zro2_10_seed_fluorite_fragment.json"
MASTER_GIN_TEMPLATE="${INPUT_DIR}/Master.gin"
RUN_JOB_TEMPLATE="${INPUT_DIR}/run.job"
MANIFEST_PATH="${RUN_DIR}/manifest.json"
PREP_ONLY="${PREP_ONLY:-0}"
GULP_BIN="${GULP_BIN:-}"

if [[ -z "${GULP_BIN}" && "${PREP_ONLY}" != "1" ]]; then
  echo "GULP_BIN must point to a runnable GULP executable." >&2
  echo "Set PREP_ONLY=1 if you only want to materialize the tracked input bundle." >&2
  exit 1
fi

mkdir -p "${INPUT_DIR}" "${WORKDIR}"

cat > "${RUN_JOB_TEMPLATE}" <<RUNJOB
# Literature-oriented staged evaluator contract for (ZrO2)10.
# Pair-potential values are aligned with the rigid-ion Buckingham terms commonly reused for ZrO2
# in GULP-based studies. Two relaxation attempts with a modest GNORM gate keep the search cheap
# while still rejecting clearly poor offspring.
N_DEF_ENERGY:GULP
N_RELAXATION_ATTEMPTS:2
GNORM:${SCOTT_GNORM}
ONLY_2ND_ENERGY:T
RUNJOB

python3 - <<'PY' "${BASE_CANDIDATE_JSON}" "${MASTER_GIN_TEMPLATE}" "${SEED}" "${GULP_GTOL}" "${GULP_GMAX}" "${GULP_FTOL}" "${GULP_XTOL}" "${GULP_MAXCYC}" "${GULP_MINIMIZER}"
from __future__ import annotations

import json
import math
import random
import sys
from pathlib import Path

out_path = Path(sys.argv[1])
master_gin_path = Path(sys.argv[2])
rng = random.Random(int(sys.argv[3]))
gtol = sys.argv[4]
gmax = sys.argv[5]
ftol = sys.argv[6]
xtol = sys.argv[7]
maxcyc = sys.argv[8]
minimizer = sys.argv[9]

a = 5.09
center = (1.5 * a, 1.5 * a, 1.5 * a)
zr_basis = [
    (0.0, 0.0, 0.0),
    (0.0, 0.5, 0.5),
    (0.5, 0.0, 0.5),
    (0.5, 0.5, 0.0),
]
o_basis = [
    (0.25, 0.25, 0.25),
    (0.25, 0.25, 0.75),
    (0.25, 0.75, 0.25),
    (0.25, 0.75, 0.75),
    (0.75, 0.25, 0.25),
    (0.75, 0.25, 0.75),
    (0.75, 0.75, 0.25),
    (0.75, 0.75, 0.75),
]


def dist2(p: tuple[float, float, float], q: tuple[float, float, float]) -> float:
    return sum((a - b) ** 2 for a, b in zip(p, q))


def dist(p: tuple[float, float, float], q: tuple[float, float, float]) -> float:
    return math.sqrt(dist2(p, q))


zrs: list[tuple[float, float, float]] = []
osites: list[tuple[float, float, float]] = []
for i in range(3):
    for j in range(3):
        for k in range(3):
            origin = (i * a, j * a, k * a)
            for basis in zr_basis:
                zrs.append(
                    (
                        origin[0] + basis[0] * a,
                        origin[1] + basis[1] * a,
                        origin[2] + basis[2] * a,
                    )
                )
            for basis in o_basis:
                osites.append(
                    (
                        origin[0] + basis[0] * a,
                        origin[1] + basis[1] * a,
                        origin[2] + basis[2] * a,
                    )
                )

selected_zr = sorted(zrs, key=lambda pos: dist2(pos, center))[:10]

ranked_o = []
for pos in osites:
    coordination = sum(1 for zr in selected_zr if dist(pos, zr) <= 2.35)
    if coordination == 0:
        continue
    ranked_o.append((-coordination, dist2(pos, center), pos))

selected_o = [pos for _, _, pos in sorted(ranked_o)[:20]]

assert len(selected_zr) == 10
assert len(selected_o) == 20

cluster_rows: list[tuple[str, tuple[float, float, float]]] = [
    *[("Zr", pos) for pos in selected_zr],
    *[("O", pos) for pos in selected_o],
]
cluster_rows.sort(key=lambda item: (item[1][2], item[1][1], item[1][0], item[0]))

mins = [min(pos[axis] for _, pos in cluster_rows) for axis in range(3)]
coords: list[list[float]] = []
species: list[str] = []
for atom, pos in cluster_rows:
    jitter = 0.03 if atom == "Zr" else 0.04
    shifted = [
        round(pos[axis] - mins[axis] + 0.2 + rng.uniform(-jitter, jitter), 6)
        for axis in range(3)
    ]
    species.append(atom)
    coords.append(shifted)

payload = {
    "label": "zro2_10_seed_fluorite_fragment",
    "species": species,
    "fractional_coords": coords,
    "lattice": None,
    "periodic_axes": [False, False, False],
}
out_path.write_text(json.dumps(payload, indent=2) + "\n")

gin_lines = [
    f"opti conv {minimizer} nosymm",
    f"gtol opt {gtol}",
    f"gmax opt {gmax}",
    f"ftol opt {ftol}",
    f"xtol opt {xtol}",
    f"maxcyc opt {maxcyc}",
    "cartesian",
]
for atom, coord in zip(species, coords):
    charge = "4.0" if atom == "Zr" else "-2.0"
    gin_lines.append(
        f"{atom:<2} core {coord[0]:10.6f} {coord[1]:10.6f} {coord[2]:10.6f} {charge} 1.0 0.0"
    )
gin_lines.extend(
    [
        "species",
        "Zr core  4.00000",
        "O  core -2.00000",
        "buckingham",
        "Zr core O core  7290.347 0.2610  0.00 0.0 12.0",
        "O  core O core    25.410 0.6937 32.32 0.0 12.0",
    ]
)
master_gin_path.write_text("\n".join(gin_lines) + "\n")
PY

cat > "${MANIFEST_PATH}" <<JSON
{
  "artifacts": {
    "base_candidate": "inputs/$(basename "${BASE_CANDIDATE_JSON}")",
    "gulp_template": "inputs/Master.gin",
    "scott_run_job": "inputs/run.job",
    "controller_trace": "traces/controller_trace.csv",
    "ga_generation_boundary_state": "raw/ga_generation_boundary_state.json",
    "ga_generation_boundary_state_latest": "raw/ga_generation_boundary_state_latest.json",
    "ga_generation_state": "raw/ga_generation_state.json",
    "ga_generation_state_latest": "raw/ga_generation_state_latest.json",
    "generation_boundary_state_per_generation": "raw/generation_XXXX_boundary_state.json",
    "generation_checkpoints": "raw/checkpoints/rust_ga_checkpoint_gen_XXXX.json",
    "generation_failure_summary_per_generation": "raw/generation_XXXX_failure_summary.json",
    "generation_metrics": "traces/generation_metrics.csv",
    "generation_origin_metrics_per_generation": "raw/generation_XXXX_origin_metrics.json",
    "generation_responses": "raw/generation_XXXX_responses.json",
    "generation_state_per_generation": "raw/generation_XXXX_state.json",
    "generation_summary_per_generation": "raw/generation_XXXX_summary.json",
    "latest_restart_checkpoint": "raw/rust_ga_checkpoint_latest.json",
    "origin_metrics": "traces/origin_metrics.csv",
    "population_snapshots": "outputs/ga_population_snapshots/generation_XXXX/",
    "rust_ga_checkpoint": "raw/rust_ga_checkpoint.json",
    "search_summary": "raw/search_summary.json",
    "structures": "outputs/structures/",
    "top_unique_candidates": "outputs/top_unique_candidates/"
  },
  "backend": "scott_runtime",
  "lane_mode": "standalone_capable",
  "parallel_contract": "staged_scott_runtime_generation_dispatch",
  "population_size": ${POPULATION},
  "provenance": {
    "backend_id": "scott_runtime",
    "backend_mode": "gulp",
    "candidate_json": "${BASE_CANDIDATE_JSON}",
    "manifest_state": "prepared",
    "run_dir": "${RUN_DIR}",
    "template_bundle": {
      "master_gin": "${MASTER_GIN_TEMPLATE}",
      "run_job": "${RUN_JOB_TEMPLATE}"
    },
    "workdir": "${WORKDIR}",
    "workflow_context": "new-run",
    "workflow_family": "Rust-owned GA",
    "workflow_id": "ga.scott-staged",
    "workflow_owner": "scott_staged_ga",
    "workflow_route": "run-ga scott-staged"
  },
  "requested_generations": ${GENERATIONS},
  "run_name": "${RUN_NAME}",
  "search_config": {
    "seed": ${SEED},
    "step_size": ${STEP_SIZE},
    "temperature": ${TEMPERATURE}
  },
  "system": "${SYSTEM_LABEL}",
  "workflow_id": "ga.scott-staged",
  "workflow_owner": "scott_staged_ga",
  "workflow_scope": "new-run"
}
JSON

echo "Run directory:  ${RUN_DIR}"
echo "Input bundle:   ${INPUT_DIR}"
echo "Workdir:        ${WORKDIR}"
echo "System:         ${SYSTEM_LABEL}"
echo "Generations:    ${GENERATIONS}"
echo "Population:     ${POPULATION}"
echo "Seed:           ${SEED}"
echo "Step size:      ${STEP_SIZE}"
echo "Scott GNORM:    ${SCOTT_GNORM}"
echo "GULP gtol:      ${GULP_GTOL}"
echo "GULP gmax:      ${GULP_GMAX}"
echo "GULP ftol:      ${GULP_FTOL}"
echo "GULP xtol:      ${GULP_XTOL}"
echo "GULP maxcyc:    ${GULP_MAXCYC}"
if [[ -n "${GULP_BIN}" ]]; then
  echo "GULP binary:    ${GULP_BIN}"
fi

if [[ "${PREP_ONLY}" == "1" ]]; then
  echo
  echo "Prepared tracked input bundle only."
  echo "Inspect:"
  echo "  ${BASE_CANDIDATE_JSON}"
  echo "  ${MASTER_GIN_TEMPLATE}"
  echo "  ${RUN_JOB_TEMPLATE}"
  exit 0
fi

echo
(
  cd "${REPO_ROOT}"
  cargo run -p patina-driver -- run-scott-staged-ga \
    --base-candidate-json "${BASE_CANDIDATE_JSON}" \
    --workdir "${WORKDIR}" \
    --run-dir "${RUN_DIR}" \
    --scott-input-dir "${INPUT_DIR}" \
    --system "${SYSTEM_LABEL}" \
    --ga-generations "${GENERATIONS}" \
    --population "${POPULATION}" \
    --seed "${SEED}" \
    --temperature "${TEMPERATURE}" \
    --step-size "${STEP_SIZE}" \
    --runtime-default-backend gulp \
    --executable "${GULP_BIN}"
)

echo
echo "Tracked run finished: ${RUN_DIR}"
echo "Watch in app from:    runs/active/${RUN_NAME}"
echo "Useful files:"
echo "  ${RUN_DIR}/manifest.json"
echo "  ${RUN_DIR}/raw/search_summary.json"
echo "  ${RUN_DIR}/raw/generation_0000_state.json"
echo "  ${RUN_DIR}/outputs/top_unique_candidates"
