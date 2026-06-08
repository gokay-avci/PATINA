#!/bin/bash -l
#$ -P Gold
#$ -A UCL_chemM_Woodley
#$ -l h_rt=01:00:00
#$ -l mem=1G
#$ -N patina_young_demo
#$ -cwd

set -euo pipefail

REPO_ROOT="${REPO_ROOT:-${SGE_O_WORKDIR:-$PWD}}"
BASE_ROOT="${PATINA_YOUNG_ROOT:-/home/uccagav/Scratch/UCL/04_APR_26}"
STAMP="$(date +%Y%m%d_%H%M%S)"
RUN_ROOT="${BASE_ROOT}/patina_young_gulp_batch_${JOB_ID:-manual}_${STAMP}"
INPUT_DIR="${RUN_ROOT}/inputs"
CANDIDATE_DIR="${INPUT_DIR}/candidates"
WORKDIR="${RUN_ROOT}/workdir"
SUMMARY_JSON="${RUN_ROOT}/batch_summary.json"
MASTER_GIN_TEMPLATE="${INPUT_DIR}/Master.gin"
FAKE_GULP_BIN="${RUN_ROOT}/fake_gulp.sh"
CARGO_TARGET_DIR="${CARGO_TARGET_DIR:-${BASE_ROOT}/cargo_target}"
BINARY="${CARGO_TARGET_DIR}/release/patina-driver"

CANDIDATE_COUNT="${CANDIDATE_COUNT:-3}"
WORKERS="${PATINA_WORKERS:-1}"
FAKE_GULP_SLEEP_SECS="${FAKE_GULP_SLEEP_SECS:-0.20}"
FAKE_GULP_SLEEP_JITTER_SECS="${FAKE_GULP_SLEEP_JITTER_SECS:-0.05}"

mkdir -p "${CANDIDATE_DIR}" "${WORKDIR}" "${CARGO_TARGET_DIR}"

module unload -f compilers mpi gcc-libs || true
module load beta-modules
module load gcc-libs/10.2.0
module load python/3.9.6

export RUST_LOG="${RUST_LOG:-info}"
export RUST_BACKTRACE="${RUST_BACKTRACE:-1}"
export PYTHONUNBUFFERED=1
export CARGO_TARGET_DIR

echo "================================================="
echo "   PATINA Young GULP Batch Demo"
echo "   Host:        $(hostname)"
echo "   Date:        $(date)"
echo "   Repo root:   ${REPO_ROOT}"
echo "   Base root:   ${BASE_ROOT}"
echo "   Run root:    ${RUN_ROOT}"
echo "   Target dir:  ${CARGO_TARGET_DIR}"
echo "   Workers:     ${WORKERS}"
echo "   Candidates:  ${CANDIDATE_COUNT}"
echo "================================================="

cat > "${MASTER_GIN_TEMPLATE}" <<'GIN'
opti conp
cartesian
# KLMC3-RS EXPECT_ATOMS: 2
# === KLMC3-RS COORDINATE INJECTION POINT ===
GIN

for index in $(seq 0 $((CANDIDATE_COUNT - 1))); do
  label="$(printf 'young_dummy_%04d' "${index}")"
  offset="$(awk -v value="${index}" 'BEGIN { printf "%.3f", (value % 11) / 100.0 }')"
  cat > "${CANDIDATE_DIR}/${label}.json" <<JSON
{
  "label": "${label}",
  "species": ["Ce", "O"],
  "fractional_coords": [
    [0.0, 0.0, 0.0],
    [${offset}, 0.5, 0.5]
  ],
  "lattice": null,
  "periodic_axes": [false, false, false]
}
JSON
done

cat > "${FAKE_GULP_BIN}" <<'SH'
#!/usr/bin/env bash
set -euo pipefail

input_name="${1:-candidate.gin}"
worker_name="$(basename "$(pwd)")"
run_name="$(basename "$(dirname "$(pwd)")")"
worker_digits="${worker_name##*_}"
worker_index=$((10#${worker_digits:-0}))

sleep_delay="$(
  awk \
    -v base="${FAKE_GULP_SLEEP_SECS:-0.20}" \
    -v jitter="${FAKE_GULP_SLEEP_JITTER_SECS:-0.05}" \
    -v slot="${worker_index}" \
    'BEGIN { printf "%.2f", base + ((slot % 4) * jitter) }'
)"

cat > run.started <<META
pid=$$
job_id=${JOB_ID:-unknown}
run=${run_name}
worker=${worker_name}
started_at=$(date -u +%Y-%m-%dT%H:%M:%SZ)
input=${input_name}
delay_secs=${sleep_delay}
META

sleep "${sleep_delay}"

energy="$(
  cksum "${input_name}" | awk '{ printf "-%.6f", ($1 % 250000) / 1000.0 }'
)"

cat > candidate.got <<GOUT
Final energy = ${energy} eV
Optimisation achieved
Final fractional coordinates of atoms
 1 Ce 0.0 0.0 0.0
 2 O 0.5 0.5 0.5
GOUT

cat > run.finished <<META
pid=$$
job_id=${JOB_ID:-unknown}
run=${run_name}
worker=${worker_name}
finished_at=$(date -u +%Y-%m-%dT%H:%M:%SZ)
energy=${energy}
META
SH

chmod +x "${FAKE_GULP_BIN}"

echo "[$(date)] Building patina-driver in release mode..."
(
  cd "${REPO_ROOT}"
  # The workspace default-members intentionally exclude `patina-llm` and `patina-tui`
  # on HPC-facing builds, but Young parity should still target the driver explicitly.
  cargo build --release -p patina-driver
)

candidate_args=()
for candidate_json in "${CANDIDATE_DIR}"/*.json; do
  candidate_args+=(--candidate-json "${candidate_json}")
done

echo "[$(date)] Running batch orchestration with fake GULP backend..."
"${BINARY}" evaluate-backend-batch \
  --backend gulp \
  "${candidate_args[@]}" \
  --workdir "${WORKDIR}" \
  --runner-mode transient \
  --workers "${WORKERS}" \
  --keep-dirs \
  --executable "${FAKE_GULP_BIN}" \
  --master-gin-template "${MASTER_GIN_TEMPLATE}" \
  | tee "${SUMMARY_JSON}"

echo
echo "Young batch workflow finished."
echo "Inspect next:"
echo "  summary:      ${SUMMARY_JSON}"
echo "  workdir:      ${WORKDIR}"
echo "  run root:     ${RUN_ROOT}"
echo
echo "Useful commands:"
echo "  find \"${WORKDIR}\" -maxdepth 2 -type d | sort"
echo "  find \"${WORKDIR}\" -name run.started | wc -l"
echo "  find \"${WORKDIR}\" -name run.finished | wc -l"
echo "  find \"${WORKDIR}\" -name candidate.got | wc -l"
echo "  rg -n 'started_at|finished_at|energy=' \"${WORKDIR}\""
