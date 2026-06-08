#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "${SCRIPT_DIR}/../.." && pwd)"
STAMP="$(date +%Y%m%d_%H%M%S)"

GENERATIONS="${GENERATIONS:-100}"
POPULATION="${POPULATION:-50}"
SEED="${SEED:-25025}"

# Duplicate pressure in the current staged Rust GA comes mainly from too much
# replacement around too few winners, combined with offspring that relax back
# into the same ionic basins. This profile lowers selection/reinsertion pressure
# and increases structural displacement so accepted children are more distinct.
STEP_SIZE="${STEP_SIZE:-0.20}"
TEMPERATURE="${TEMPERATURE:-10.0}"

TOURNAMENT_SIZE_MIN="${TOURNAMENT_SIZE_MIN:-2}"
TOURNAMENT_SIZE_MAX="${TOURNAMENT_SIZE_MAX:-6}"
POP_REPLACEMENT_RATIO="${POP_REPLACEMENT_RATIO:-0.50}"
REINSERT_ELITES_RATIO="${REINSERT_ELITES_RATIO:-0.20}"
MUTATION_RATIO="${MUTATION_RATIO:-0.40}"
MUT_SELFCROSS_RATIO="${MUT_SELFCROSS_RATIO:-0.55}"
CROSSOVER_ATTEMPTS="${CROSSOVER_ATTEMPTS:-6}"
MAX_REPOP_ATTEMPTS="${MAX_REPOP_ATTEMPTS:-12}"

SCOTT_GNORM="${SCOTT_GNORM:-0.20}"
GULP_GTOL="${GULP_GTOL:-${SCOTT_GNORM}}"
GULP_GMAX="${GULP_GMAX:-${SCOTT_GNORM}}"
GULP_FTOL="${GULP_FTOL:-0.2}"
GULP_XTOL="${GULP_XTOL:-0.2}"
GULP_MAXCYC="${GULP_MAXCYC:-350}"
GULP_MINIMIZER="${GULP_MINIMIZER:-lbfgs}"

SYSTEM_LABEL="${SYSTEM_LABEL:-'(MgO)25'}"
RUN_NAME="${RUN_NAME:-mgo25_staged_gulp_lowrepop_100g120p_${STAMP}}"
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
# Low-repopulation staged evaluator contract for (MgO)25.
# The controller-side diversity settings are passed on the Rust CLI below.
N_DEF_ENERGY:GULP
N_RELAXATION_ATTEMPTS:2
GNORM:${SCOTT_GNORM}
ONLY_2ND_ENERGY:T
RUNJOB

python3 - <<'PY' "${BASE_CANDIDATE_JSON}" "${MASTER_GIN_TEMPLATE}" "${SEED}" "${GULP_GTOL}" "${GULP_GMAX}" "${GULP_FTOL}" "${GULP_XTOL}" "${GULP_MAXCYC}" "${GULP_MINIMIZER}"
from __future__ import annotations

import json
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

a = 2.106
species = []
coords = []
for i in range(5):
    for j in range(5):
        for k in range(2):
            species.append("Mg" if (i + j + k) % 2 == 0 else "O")
            jitter = [
                rng.uniform(-0.035, 0.035),
                rng.uniform(-0.035, 0.035),
                rng.uniform(-0.035, 0.035),
            ]
            coords.append(
                [
                    round(i * a + 0.5 * a * (j % 2) + jitter[0], 6),
                    round(j * a + jitter[1], 6),
                    round(k * a + 0.5 * a * ((i + j) % 2) + jitter[2], 6),
                ]
            )

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
        "O  core O core  22764.0000  0.1490  27.88   0.0  12.0",
    ]
)
master_gin_path.write_text("\n".join(gin_lines) + "\n")
PY

echo "Run directory:            ${RUN_DIR}"
echo "Population / generations: ${POPULATION} / ${GENERATIONS}"
echo "Step size:                ${STEP_SIZE}"
echo "Tournament:               ${TOURNAMENT_SIZE_MIN}-${TOURNAMENT_SIZE_MAX}"
echo "Replacement ratio:        ${POP_REPLACEMENT_RATIO}"
echo "Elite reinsertion ratio:  ${REINSERT_ELITES_RATIO}"
echo "Mutation ratio:           ${MUTATION_RATIO}"
echo "Mut selfcross ratio:      ${MUT_SELFCROSS_RATIO}"
echo "Crossover attempts:       ${CROSSOVER_ATTEMPTS}"
echo "Max repop attempts:       ${MAX_REPOP_ATTEMPTS}"
echo "GULP binary:              ${GULP_BIN}"
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
    --executable "${GULP_BIN}" \
    --tournament-size-min "${TOURNAMENT_SIZE_MIN}" \
    --tournament-size-max "${TOURNAMENT_SIZE_MAX}" \
    --pop-replacement-ratio "${POP_REPLACEMENT_RATIO}" \
    --reinsert-elites-ratio "${REINSERT_ELITES_RATIO}" \
    --mutation-ratio "${MUTATION_RATIO}" \
    --mut-selfcross-ratio "${MUT_SELFCROSS_RATIO}" \
    --crossover-attempts "${CROSSOVER_ATTEMPTS}" \
    --max-repop-attempts "${MAX_REPOP_ATTEMPTS}"
)

echo
echo "Tracked run finished: ${RUN_DIR}"
