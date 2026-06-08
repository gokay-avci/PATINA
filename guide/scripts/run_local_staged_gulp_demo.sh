#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "${SCRIPT_DIR}/../.." && pwd)"
STAMP="$(date +%Y%m%d_%H%M%S)"
DEMO_ROOT="${TMPDIR:-/tmp}/patina_staged_gulp_demo_${STAMP}"
INPUT_DIR="${DEMO_ROOT}/inputs"
WORKDIR="${DEMO_ROOT}/workdir"
RUN_DIR="${DEMO_ROOT}/run"

mkdir -p "${INPUT_DIR}" "${WORKDIR}" "${RUN_DIR}"

cat > "${INPUT_DIR}/candidate.json" <<'JSON'
{
  "label": "demo_ceria_cluster",
  "species": ["Ce", "O"],
  "fractional_coords": [
    [0.0, 0.0, 0.0],
    [0.5, 0.5, 0.5]
  ],
  "lattice": null,
  "periodic_axes": [false, false, false]
}
JSON

cat > "${INPUT_DIR}/Master.gin" <<'GIN'
opti conp
cartesian
# KLMC3-RS EXPECT_ATOMS: 2
# === KLMC3-RS COORDINATE INJECTION POINT ===
GIN

cat > "${INPUT_DIR}/run.job" <<'RUNJOB'
N_DEF_ENERGY:GULP
RUNJOB

cat > "${DEMO_ROOT}/fake_gulp.sh" <<'SH'
#!/usr/bin/env bash
set -euo pipefail
cat > gulp_klmc.gout <<'GOUT'
Final energy = -1.23 eV
Optimisation achieved
Final fractional coordinates of atoms
 1 Ce 0.0 0.0 0.0
 2 O 0.5 0.5 0.5
GOUT
SH
chmod +x "${DEMO_ROOT}/fake_gulp.sh"

echo "Demo root: ${DEMO_ROOT}"
echo "Input dir:  ${INPUT_DIR}"
echo "Workdir:    ${WORKDIR}"
echo "Run dir:    ${RUN_DIR}"
echo
echo "Executing staged Scott runtime with a local fake GULP backend..."
echo

(
  cd "${REPO_ROOT}"
  cargo run -p patina-driver -- evaluate-scott-staged \
    --candidate-json "${INPUT_DIR}/candidate.json" \
    --workdir "${WORKDIR}" \
    --run-dir "${RUN_DIR}" \
    --scott-input-dir "${INPUT_DIR}" \
    --runtime-default-backend gulp \
    --executable "${DEMO_ROOT}/fake_gulp.sh"
)

echo
echo "Workflow finished."
echo "Inspect these next:"
echo "  result summary stdout above"
echo "  staged workdir: ${WORKDIR}"
echo "  tracked artifacts: ${RUN_DIR}"
echo
echo "Most useful files:"
echo "  ${WORKDIR}/stage_01_attempt_01/gulp_klmc.gin"
echo "  ${WORKDIR}/stage_01_attempt_01/gulp_klmc.gout"
echo "  ${RUN_DIR}/raw/single_eval_summary.json"
