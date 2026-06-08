#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "${SCRIPT_DIR}/../.." && pwd)"
STAMP="$(date +%Y%m%d_%H%M%S)"

GENERATIONS="${GENERATIONS:-100}"
POPULATION="${POPULATION:-100}"
SEED="${SEED:-25025}"
STEP_SIZE="${STEP_SIZE:-0.35}"
TEMPERATURE="${TEMPERATURE:-10.0}"
SCOTT_GNORM="${SCOTT_GNORM:-0.2}"
GULP_GTOL="${GULP_GTOL:-${SCOTT_GNORM}}"
GULP_GMAX="${GULP_GMAX:-${SCOTT_GNORM}}"
GULP_FTOL="${GULP_FTOL:-0.001}"
GULP_XTOL="${GULP_XTOL:-0.001}"
GULP_MAXCYC="${GULP_MAXCYC:-250}"
GULP_MINIMIZER="${GULP_MINIMIZER:-lbfgs}"
SYSTEM_LABEL="${SYSTEM_LABEL:-'(MgO)25'}"
RUN_NAME="${RUN_NAME:-mgo25_staged_gulp_${GENERATIONS}g${POPULATION}p_${STAMP}}"
RUN_DIR="${RUN_DIR:-${REPO_ROOT}/runs/active/${RUN_NAME}}"
INPUT_DIR="${RUN_DIR}/inputs"
WORKDIR="${RUN_DIR}/workdir"
BASE_CANDIDATE_JSON="${INPUT_DIR}/mgo25_seed_rocksalt_fragment.json"
MASTER_GIN_TEMPLATE="${INPUT_DIR}/Master.gin"
RUN_JOB_TEMPLATE="${INPUT_DIR}/run.job"
GULP_BIN="${GULP_BIN:-}"

if [[ -z "${GULP_BIN}" ]]; then
  echo "GULP_BIN must point to a runnable GULP executable." >&2
  exit 1
fi

mkdir -p "${INPUT_DIR}" "${WORKDIR}"

cat > "${RUN_JOB_TEMPLATE}" <<RUNJOB
# Fast exploration-oriented staged evaluator contract for (MgO)25.
# One stage keeps the run cheap; a loose gnorm gate avoids rejecting too many
# partially relaxed offspring during a 100 x 100 exploratory campaign.
N_DEF_ENERGY:GULP
N_RELAXATION_ATTEMPTS:1
GNORM:${SCOTT_GNORM}
ONLY_2ND_ENERGY:F
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

# A compact 5 x 5 x 2 rocksalt fragment gives exactly 50 sites, hence 25 Mg and 25 O.
# The small random jitter prevents a perfectly symmetric starting seed from dominating
# early duplicate filtering while keeping the motif recognisably rocksalt-like.
a = 2.106
species = []
coords = []
for i in range(5):
    for j in range(5):
        for k in range(2):
            species.append("Mg" if (i + j + k) % 2 == 0 else "O")
            jitter = [
                rng.uniform(-0.04, 0.04),
                rng.uniform(-0.04, 0.04),
                rng.uniform(-0.04, 0.04),
            ]
            coords.append(
                [
                    round(i * a + 0.5 * a * (j % 2) + jitter[0], 6),
                    round(j * a + jitter[1], 6),
                    round(k * a + 0.5 * a * ((i + j) % 2) + jitter[2], 6),
                ]
            )

assert species.count("Mg") == 25
assert species.count("O") == 25

payload = {
    "label": "mgo25_seed_rocksalt_fragment",
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
for specie, coord in zip(species, coords):
    charge = "2.0" if specie == "Mg" else "-2.0"
    gin_lines.append(
        f"{specie:<2} core {coord[0]:10.6f} {coord[1]:10.6f} {coord[2]:10.6f} {charge} 1.0 0.0"
    )
gin_lines.extend(
    [
        "species",
        "Mg core  2.00000",
        "O  core -2.00000",
        "buckingham",
        "Mg core O core  1428.500 0.2945  0.00 0.0 12.0",
        "O  core O core    22.410 0.6937 32.32 0.0 12.0",
    ]
)
master_gin_path.write_text("\n".join(gin_lines) + "\n")
PY

echo "Run directory:  ${RUN_DIR}"
echo "Input bundle:   ${INPUT_DIR}"
echo "Workdir:        ${WORKDIR}"
echo "GULP binary:    ${GULP_BIN}"
echo "Minimizer:      ${GULP_MINIMIZER}"
echo "Scott GNORM:    ${SCOTT_GNORM}"
echo "GULP gtol:      ${GULP_GTOL}"
echo "GULP gmax:      ${GULP_GMAX}"
echo "GULP ftol:      ${GULP_FTOL}"
echo "GULP xtol:      ${GULP_XTOL}"
echo "GULP maxcyc:    ${GULP_MAXCYC}"
echo "Generations:    ${GENERATIONS}"
echo "Population:     ${POPULATION}"
echo "Seed:           ${SEED}"
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
