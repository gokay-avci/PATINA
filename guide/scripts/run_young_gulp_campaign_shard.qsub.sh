#!/bin/bash -l
#$ -P Gold
#$ -A UCL_chemM_Woodley
#$ -l h_rt=01:00:00
#$ -l mem=1G
#$ -N patina_young_shard
#$ -cwd

set -euo pipefail

REPO_ROOT="${REPO_ROOT:-${SGE_O_WORKDIR:-$PWD}}"
BASE_ROOT="${PATINA_YOUNG_ROOT:-/home/uccagav/Scratch/UCL/04_APR_26}"
CAMPAIGN_ROOT="${PATINA_YOUNG_CAMPAIGN_ROOT:-}"
if [ -z "${CAMPAIGN_ROOT}" ]; then
  echo "PATINA_YOUNG_CAMPAIGN_ROOT must point to a shared campaign root" >&2
  exit 1
fi
SHARD_ID="${PATINA_SHARD_ID:-}"
if [ -z "${SHARD_ID}" ]; then
  echo "PATINA_SHARD_ID must be set" >&2
  exit 1
fi
STAMP="$(date +%Y%m%d_%H%M%S)"
RUN_ROOT="${CAMPAIGN_ROOT}/runs/${SHARD_ID}_${JOB_ID:-manual}_${STAMP}"
INPUT_DIR="${RUN_ROOT}/inputs"
CANDIDATE_DIR="${INPUT_DIR}/candidates"
WORKDIR="${RUN_ROOT}/workdir"
SUMMARY_JSON="${RUN_ROOT}/batch_summary.json"
MASTER_GIN_TEMPLATE="${INPUT_DIR}/Master.gin"
FAKE_GULP_BIN="${RUN_ROOT}/fake_gulp.sh"
CARGO_TARGET_DIR="${CARGO_TARGET_DIR:-${BASE_ROOT}/cargo_target}"
BINARY="${CARGO_TARGET_DIR}/release/patina-driver"

PROJECTION_ROOT="${CAMPAIGN_ROOT}/projections"
RECEIPTS_DIR="${PROJECTION_ROOT}/receipts"
STATES_DIR="${PROJECTION_ROOT}/states"
HEALTH_DIR="${CAMPAIGN_ROOT}/health"
METRICS_DIR="${CAMPAIGN_ROOT}/metrics"
SHARDS_DIR="${CAMPAIGN_ROOT}/shards"
SUBMISSIONS_DIR="${CAMPAIGN_ROOT}/submissions"

RECEIPT_ID="grid_engine:${JOB_ID:-manual}:${SHARD_ID}"
LOGICAL_JOB_ID="campaign-shard:${SHARD_ID}"
RECEIPT_BASENAME="$(printf '%s' "${RECEIPT_ID}" | xxd -p -c 256)"
STATE_BASENAME="$(printf '%s' "${LOGICAL_JOB_ID}" | xxd -p -c 256)"
RECEIPT_PATH="${RECEIPTS_DIR}/${RECEIPT_BASENAME}.json"
STATE_PATH="${STATES_DIR}/${STATE_BASENAME}.json"
HEARTBEAT_PATH="${HEALTH_DIR}/${SHARD_ID}.heartbeat.json"
TERMINAL_SUMMARY_PATH="${HEALTH_DIR}/${SHARD_ID}.terminal_summary.json"
METRICS_PATH="${METRICS_DIR}/${SHARD_ID}.json"
SUBMISSION_NOTE_PATH="${SUBMISSIONS_DIR}/${SHARD_ID}.runtime.txt"

CANDIDATE_COUNT="${PATINA_CANDIDATE_COUNT:-3}"
CANDIDATE_OFFSET="${PATINA_CANDIDATE_OFFSET:-0}"
WORKERS="${PATINA_WORKERS:-1}"
FAKE_GULP_SLEEP_SECS="${FAKE_GULP_SLEEP_SECS:-0.20}"
FAKE_GULP_SLEEP_JITTER_SECS="${FAKE_GULP_SLEEP_JITTER_SECS:-0.05}"

CURRENT_RECEIPT_STATE="submitted"
CURRENT_ORCH_STATUS="materializing"
CURRENT_FAILURE_KIND="null"
CURRENT_MESSAGE="initializing shard"
CURRENT_ATTEMPT=1
CURRENT_ROOT_PID=""

mkdir -p \
  "${CANDIDATE_DIR}" \
  "${WORKDIR}" \
  "${CARGO_TARGET_DIR}" \
  "${RECEIPTS_DIR}" \
  "${STATES_DIR}" \
  "${HEALTH_DIR}" \
  "${METRICS_DIR}" \
  "${SHARDS_DIR}" \
  "${SUBMISSIONS_DIR}"

module unload -f compilers mpi gcc-libs || true
module load beta-modules
module load gcc-libs/10.2.0
module load python/3.9.6

export RUST_LOG="${RUST_LOG:-info}"
export RUST_BACKTRACE="${RUST_BACKTRACE:-1}"
export PYTHONUNBUFFERED=1
export CARGO_TARGET_DIR

now_ms() {
  echo $(( $(date +%s) * 1000 ))
}

json_escape() {
  printf '%s' "$1" | python3 -c 'import json,sys; print(json.dumps(sys.stdin.read()))'
}

write_metrics_snapshot() {
  local phase="$1"
  local sampled_at_ms
  local host
  local rss_total_mb
  local rss_max_mb
  local vm_total_mb
  local cpu_percent_total
  local gpu_util_percent
  local gpu_memory_mb
  local job_scope_json

  sampled_at_ms="$(now_ms)"
  host="$(hostname)"
  rss_total_mb="$(ps -eo rss= | awk '{sum += $1} END {printf "%.0f", sum / 1024}')"
  rss_max_mb="$(ps -eo rss= | awk 'BEGIN {max = 0} {if ($1 > max) max = $1} END {printf "%.0f", max / 1024}')"
  vm_total_mb="$(ps -eo vsz= | awk '{sum += $1} END {printf "%.0f", sum / 1024}')"
  cpu_percent_total="$(ps -eo pcpu= | awk '{sum += $1} END {printf "%.1f", sum}')"
  gpu_util_percent="null"
  gpu_memory_mb="null"

  if command -v nvidia-smi >/dev/null 2>&1; then
    local gpu_sample
    gpu_sample="$(nvidia-smi --query-gpu=utilization.gpu,memory.used --format=csv,noheader,nounits 2>/dev/null | head -n 1 || true)"
    if [ -n "${gpu_sample}" ]; then
      gpu_util_percent="$(printf '%s' "${gpu_sample}" | awk -F',' '{gsub(/ /, "", $1); print $1}')"
      gpu_memory_mb="$(printf '%s' "${gpu_sample}" | awk -F',' '{gsub(/ /, "", $2); print $2}')"
    fi
  fi

  job_scope_json="$(
    python3 - <<'PY' "${CURRENT_ROOT_PID}" "${sampled_at_ms}" "${host}" "${gpu_util_percent}"
import json
import subprocess
import sys

root_pid = sys.argv[1].strip()
sampled_at_ms = int(sys.argv[2])
host = sys.argv[3]
gpu_raw = sys.argv[4].strip()
gpu_util = None if gpu_raw in {"", "null"} else float(gpu_raw)

def to_int(value):
    try:
        return int(float(value))
    except (TypeError, ValueError):
        return None

def to_float(value):
    try:
        return float(value)
    except (TypeError, ValueError):
        return None

root = to_int(root_pid)
rows = []
if root is not None:
    try:
        output = subprocess.check_output(
            ["ps", "-eo", "pid=,ppid=,rss=,vsz=,pcpu=,comm="],
            text=True,
        )
    except Exception:
        output = ""

    table = {}
    children = {}
    for line in output.splitlines():
        parts = line.strip().split(None, 5)
        if len(parts) < 5:
            continue
        pid = to_int(parts[0])
        ppid = to_int(parts[1])
        if pid is None:
            continue
        table[pid] = {
            "pid": pid,
            "ppid": ppid,
            "rss_mb": None if to_int(parts[2]) is None else round(to_int(parts[2]) / 1024),
            "vm_mb": None if to_int(parts[3]) is None else round(to_int(parts[3]) / 1024),
            "cpu_percent": to_float(parts[4]),
            "command": parts[5] if len(parts) > 5 else None,
        }
        if ppid is not None:
            children.setdefault(ppid, []).append(pid)

    stack = [root]
    seen = set()
    while stack:
        pid = stack.pop()
        if pid in seen:
            continue
        seen.add(pid)
        if pid in table:
            rows.append(table[pid])
        stack.extend(children.get(pid, []))

rss_values = [row["rss_mb"] for row in rows if row["rss_mb"] is not None]
vm_values = [row["vm_mb"] for row in rows if row["vm_mb"] is not None]
cpu_values = [row["cpu_percent"] for row in rows if row["cpu_percent"] is not None]
payload = {
    "scope": "job_process_tree",
    "sampled_at_ms": sampled_at_ms,
    "root_pid": root,
    "process_count": len(rows),
    "rss_total_mb": sum(rss_values) if rss_values else None,
    "rss_peak_mb": max(rss_values) if rss_values else None,
    "vm_total_mb": sum(vm_values) if vm_values else None,
    "cpu_percent_total": round(sum(cpu_values), 3) if cpu_values else None,
    "gpu_util_percent": gpu_util,
    "node_list": [host],
    "processes": rows,
}
print(json.dumps(payload, separators=(",", ":")))
PY
  )"

  cat > "${METRICS_PATH}" <<JSON
{
  "campaign_root": $(json_escape "${CAMPAIGN_ROOT}"),
  "shard_id": $(json_escape "${SHARD_ID}"),
  "job_id": $(json_escape "${JOB_ID:-manual}"),
  "phase": $(json_escape "${phase}"),
  "sampled_at_ms": ${sampled_at_ms},
  "host": $(json_escape "${host}"),
  "nslots": ${NSLOTS:-0},
  "scope": "job_scoped_with_node_visible_legacy",
  "job_scoped": ${job_scope_json},
  "node_visible_legacy": {
    "scope": "node_visible_snapshot",
    "rss_total_mb": ${rss_total_mb},
    "rss_max_mb": ${rss_max_mb},
    "vm_total_mb": ${vm_total_mb},
    "cpu_percent_total": ${cpu_percent_total}
  },
  "rss_total_mb": ${rss_total_mb},
  "rss_max_mb": ${rss_max_mb},
  "vm_total_mb": ${vm_total_mb},
  "cpu_percent_total": ${cpu_percent_total},
  "gpu_util_percent": ${gpu_util_percent},
  "gpu_memory_mb": ${gpu_memory_mb}
}
JSON
}

write_projection_files() {
  local observed_at_ms
  local host
  local receipt_message

  observed_at_ms="$(now_ms)"
  host="$(hostname)"
  receipt_message="shard ${SHARD_ID} observed on ${host}"

  cat > "${RECEIPT_PATH}" <<JSON
{
  "receipt_id": $(json_escape "${RECEIPT_ID}"),
  "job_id": $(json_escape "${LOGICAL_JOB_ID}"),
  "scheduler_job": {
    "scheduler_family": "grid_engine",
    "allocation_id": $(json_escape "${JOB_ID:-manual}"),
    "step_id": null,
    "array_job_id": null,
    "array_index": null
  },
  "state": $(json_escape "${CURRENT_RECEIPT_STATE}"),
  "submitted_at_ms": ${observed_at_ms},
  "last_observed_at_ms": ${observed_at_ms},
  "launch_host": $(json_escape "${host}"),
  "workdir": $(json_escape "${WORKDIR}"),
  "launcher_provenance": null,
  "last_telemetry": {
    "sampled_at_ms": ${observed_at_ms},
    "rss_mb": $(python3 - <<'PY' "${METRICS_PATH}"
import json, pathlib, sys
path = pathlib.Path(sys.argv[1])
if not path.exists():
    print("null")
else:
    metric = json.loads(path.read_text())
    print((metric.get("job_scoped") or {}).get("rss_total_mb") or metric.get("rss_total_mb", "null"))
PY
),
    "vm_mb": $(python3 - <<'PY' "${METRICS_PATH}"
import json, pathlib, sys
path = pathlib.Path(sys.argv[1])
if not path.exists():
    print("null")
else:
    metric = json.loads(path.read_text())
    print((metric.get("job_scoped") or {}).get("vm_total_mb") or metric.get("vm_total_mb", "null"))
PY
),
    "cpu_time_secs": null,
    "gpu_util_percent": $(python3 - <<'PY' "${METRICS_PATH}"
import json, pathlib, sys
path = pathlib.Path(sys.argv[1])
if not path.exists():
    print("null")
else:
    print(json.loads(path.read_text()).get("gpu_util_percent", "null"))
PY
),
    "node_list": [$(json_escape "${host}")]
  }
}
JSON

  cat > "${STATE_PATH}" <<JSON
{
  "job_id": $(json_escape "${LOGICAL_JOB_ID}"),
  "status": $(json_escape "${CURRENT_ORCH_STATUS}"),
  "attempt": ${CURRENT_ATTEMPT},
  "lease_id": null,
  "batch_receipt_id": $(json_escape "${RECEIPT_ID}"),
  "worker_id": $(json_escape "${host}"),
  "last_transition_ms": ${observed_at_ms},
  "failure_kind": ${CURRENT_FAILURE_KIND},
  "profiling": {
    "peak_rss_mb": $(python3 - <<'PY' "${METRICS_PATH}"
import json, pathlib, sys
path = pathlib.Path(sys.argv[1])
if not path.exists():
    print("null")
else:
    metric = json.loads(path.read_text())
    print((metric.get("job_scoped") or {}).get("rss_peak_mb") or metric.get("rss_max_mb", "null"))
PY
),
    "peak_vm_mb": $(python3 - <<'PY' "${METRICS_PATH}"
import json, pathlib, sys
path = pathlib.Path(sys.argv[1])
if not path.exists():
    print("null")
else:
    metric = json.loads(path.read_text())
    print((metric.get("job_scoped") or {}).get("vm_total_mb") or metric.get("vm_total_mb", "null"))
PY
),
    "cpu_time_secs": null,
    "wallclock_secs": null,
    "sampled_nodes": [$(json_escape "${host}")]
  }
}
JSON

  cat > "${HEARTBEAT_PATH}" <<JSON
{
  "phase": $(json_escape "${CURRENT_ORCH_STATUS}"),
  "observed_at_ms": ${observed_at_ms},
  "attempt": ${CURRENT_ATTEMPT},
  "active_job_id": $(json_escape "${LOGICAL_JOB_ID}"),
  "backend_program": "gulp",
  "message": $(json_escape "${CURRENT_MESSAGE}")
}
JSON
}

write_terminal_summary() {
  local final_phase="$1"
  local message="$2"
  local completed_job_ids
  local failed_job_ids

  if [ "${final_phase}" = "completed" ]; then
    completed_job_ids="[\"${LOGICAL_JOB_ID}\"]"
    failed_job_ids="[]"
  else
    completed_job_ids="[]"
    failed_job_ids="[\"${LOGICAL_JOB_ID}\"]"
  fi

  cat > "${TERMINAL_SUMMARY_PATH}" <<JSON
{
  "receipt_id": $(json_escape "${RECEIPT_ID}"),
  "shard_id": $(json_escape "${SHARD_ID}"),
  "observed_at_ms": $(now_ms),
  "final_phase": $(json_escape "${final_phase}"),
  "completed_job_ids": ${completed_job_ids},
  "failed_job_ids": ${failed_job_ids},
  "salvageable_job_ids": [],
  "artifact_roots": [$(json_escape "${RUN_ROOT}")],
  "message": $(json_escape "${message}")
}
JSON
}

on_error() {
  CURRENT_RECEIPT_STATE="failed"
  CURRENT_ORCH_STATUS="failed"
  CURRENT_FAILURE_KIND=$(json_escape "runtime")
  CURRENT_MESSAGE="shard execution failed"
  write_metrics_snapshot "failed"
  write_projection_files
  write_terminal_summary "failed" "shard execution failed"
}

trap on_error ERR

cat > "${SUBMISSION_NOTE_PATH}" <<NOTE
job_id=${JOB_ID:-manual}
shard_id=${SHARD_ID}
campaign_root=${CAMPAIGN_ROOT}
run_root=${RUN_ROOT}
submitted_host=$(hostname)
NOTE

echo "================================================="
echo "   PATINA Young GULP Campaign Shard"
echo "   Host:         $(hostname)"
echo "   Date:         $(date)"
echo "   Campaign:     ${CAMPAIGN_ROOT}"
echo "   Shard:        ${SHARD_ID}"
echo "   Job ID:       ${JOB_ID:-manual}"
echo "   Run root:     ${RUN_ROOT}"
echo "   Target dir:   ${CARGO_TARGET_DIR}"
echo "   Workers:      ${WORKERS}"
echo "   Candidates:   ${CANDIDATE_COUNT}"
echo "   Offset:       ${CANDIDATE_OFFSET}"
echo "================================================="

CURRENT_RECEIPT_STATE="running"
CURRENT_ORCH_STATUS="materializing"
CURRENT_MESSAGE="preparing shard inputs and projections"
write_metrics_snapshot "materializing"
write_projection_files

cat > "${MASTER_GIN_TEMPLATE}" <<'GIN'
opti conp
cartesian
# KLMC3-RS EXPECT_ATOMS: 2
# === KLMC3-RS COORDINATE INJECTION POINT ===
GIN

for index in $(seq 0 $((CANDIDATE_COUNT - 1))); do
  absolute_index=$((CANDIDATE_OFFSET + index))
  label="$(printf '%s_candidate_%04d' "${SHARD_ID}" "${absolute_index}")"
  offset="$(awk -v value="${absolute_index}" 'BEGIN { printf "%.3f", (value % 11) / 100.0 }')"
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
  cargo build --release -p patina-driver
)

candidate_args=()
for candidate_json in "${CANDIDATE_DIR}"/*.json; do
  candidate_args+=(--candidate-json "${candidate_json}")
done

CURRENT_RECEIPT_STATE="running"
CURRENT_ORCH_STATUS="running_external"
CURRENT_MESSAGE="executing fake GULP shard batch"
write_metrics_snapshot "running_external"
write_projection_files

echo "[$(date)] Running shard orchestration with fake GULP backend..."
"${BINARY}" evaluate-backend-batch \
  --backend gulp \
  "${candidate_args[@]}" \
  --workdir "${WORKDIR}" \
  --runner-mode transient \
  --workers "${WORKERS}" \
  --keep-dirs \
  --executable "${FAKE_GULP_BIN}" \
  --master-gin-template "${MASTER_GIN_TEMPLATE}" \
  > "${SUMMARY_JSON}.stdout" 2>&1 &

driver_pid=$!
CURRENT_ROOT_PID="${driver_pid}"
echo "[$(date)] Shard driver pid: ${driver_pid}"
while kill -0 "${driver_pid}" >/dev/null 2>&1; do
  write_metrics_snapshot "running_external"
  sleep 5
done
wait "${driver_pid}"
cat "${SUMMARY_JSON}.stdout" | tee "${SUMMARY_JSON}"

CURRENT_RECEIPT_STATE="completed"
CURRENT_ORCH_STATUS="completed"
CURRENT_FAILURE_KIND="null"
CURRENT_MESSAGE="shard batch completed successfully"
write_metrics_snapshot "completed"
write_projection_files
write_terminal_summary "completed" "shard batch completed successfully"

echo
echo "Young shard workflow finished."
echo "Inspect next:"
echo "  campaign root: ${CAMPAIGN_ROOT}"
echo "  summary:       ${SUMMARY_JSON}"
echo "  workdir:       ${WORKDIR}"
echo "  run root:      ${RUN_ROOT}"
echo
echo "Useful commands:"
echo "  python3 \"${REPO_ROOT}/guide/scripts/inspect_young_campaign_health.py\" \"${CAMPAIGN_ROOT}\""
echo "  find \"${WORKDIR}\" -name run.finished | wc -l"
echo "  rg -n 'started_at|finished_at|energy=' \"${WORKDIR}\""
