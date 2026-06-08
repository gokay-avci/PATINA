#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "${SCRIPT_DIR}/../.." && pwd)"
STAMP="$(date +%Y%m%d_%H%M%S)"

CANDIDATE_COUNT="${CANDIDATE_COUNT:-100}"
if [[ $# -ge 1 ]]; then
  CANDIDATE_COUNT="$1"
fi

if command -v sysctl >/dev/null 2>&1; then
  DEFAULT_WORKERS="$(sysctl -n hw.logicalcpu 2>/dev/null || echo 10)"
else
  DEFAULT_WORKERS=10
fi
WORKERS="${WORKERS:-$DEFAULT_WORKERS}"
if [[ $# -ge 2 ]]; then
  WORKERS="$2"
fi

FAKE_GULP_SLEEP_SECS="${FAKE_GULP_SLEEP_SECS:-0.75}"
FAKE_GULP_SLEEP_JITTER_SECS="${FAKE_GULP_SLEEP_JITTER_SECS:-0.15}"

DEMO_ROOT="${TMPDIR:-/tmp}/patina_gulp_batch_demo_${STAMP}"
INPUT_DIR="${DEMO_ROOT}/inputs"
CANDIDATE_DIR="${INPUT_DIR}/candidates"
WORKDIR="${DEMO_ROOT}/workdir"
SUMMARY_JSON="${DEMO_ROOT}/batch_summary.json"
MASTER_GIN_TEMPLATE="${INPUT_DIR}/Master.gin"
FAKE_GULP_BIN="${DEMO_ROOT}/fake_gulp.sh"

mkdir -p "${CANDIDATE_DIR}" "${WORKDIR}"

cat > "${MASTER_GIN_TEMPLATE}" <<'GIN'
opti conp
cartesian
# KLMC3-RS EXPECT_ATOMS: 2
# === KLMC3-RS COORDINATE INJECTION POINT ===
GIN

for index in $(seq 0 $((CANDIDATE_COUNT - 1))); do
  label="$(printf 'dummy_%04d' "${index}")"
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
    -v base="${FAKE_GULP_SLEEP_SECS:-0.75}" \
    -v jitter="${FAKE_GULP_SLEEP_JITTER_SECS:-0.15}" \
    -v slot="${worker_index}" \
    'BEGIN { printf "%.2f", base + ((slot % 4) * jitter) }'
)"

cat > run.started <<META
pid=$$
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
run=${run_name}
worker=${worker_name}
finished_at=$(date -u +%Y-%m-%dT%H:%M:%SZ)
energy=${energy}
META
SH

chmod +x "${FAKE_GULP_BIN}"

candidate_args=()
for candidate_json in "${CANDIDATE_DIR}"/*.json; do
  candidate_args+=(--candidate-json "${candidate_json}")
done

echo "Demo root:      ${DEMO_ROOT}"
echo "Candidate dir:  ${CANDIDATE_DIR}"
echo "Workdir:        ${WORKDIR}"
echo "Summary JSON:   ${SUMMARY_JSON}"
echo "Workers:        ${WORKERS}"
echo "Candidate count:${CANDIDATE_COUNT}"
echo "Fake GULP sleep:${FAKE_GULP_SLEEP_SECS}s + slot jitter ${FAKE_GULP_SLEEP_JITTER_SECS}s"
echo
echo "Running real batch orchestration through patina-driver..."
echo

(
  cd "${REPO_ROOT}"
  cargo run --quiet -p patina-driver -- evaluate-backend-batch \
    --backend gulp \
    "${candidate_args[@]}" \
    --workdir "${WORKDIR}" \
    --runner-mode transient \
    --workers "${WORKERS}" \
    --keep-dirs \
    --executable "${FAKE_GULP_BIN}" \
    --master-gin-template "${MASTER_GIN_TEMPLATE}" \
    | tee "${SUMMARY_JSON}"
)

echo
echo "Batch workflow finished."
echo "Useful inspection commands:"
echo "  find \"${WORKDIR}\" -maxdepth 2 -type d | sort"
echo "  find \"${WORKDIR}\" -name run.started | wc -l"
echo "  find \"${WORKDIR}\" -name run.finished | wc -l"
echo "  find \"${WORKDIR}\" -name candidate.got | wc -l"
echo "  rg -n 'started_at|finished_at|energy=' \"${WORKDIR}\""
echo
echo "Each batch invocation creates one run directory with isolated worker sandboxes under:"
echo "  ${WORKDIR}"
