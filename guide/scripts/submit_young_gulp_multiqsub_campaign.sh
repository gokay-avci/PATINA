#!/usr/bin/env bash
set -euo pipefail

REPO_ROOT="${REPO_ROOT:-$PWD}"
BASE_ROOT="${PATINA_YOUNG_ROOT:-/home/uccagav/Scratch/UCL/04_APR_26}"
STAMP="$(date +%Y%m%d_%H%M%S)"
CAMPAIGN_ROOT="${PATINA_YOUNG_CAMPAIGN_ROOT:-${BASE_ROOT}/patina_young_campaign_${STAMP}}"
SHARD_COUNT="${PATINA_SHARD_COUNT:-3}"
CANDIDATES_PER_SHARD="${PATINA_CANDIDATES_PER_SHARD:-3}"
WORKERS="${PATINA_WORKERS:-1}"
SHARD_SCRIPT="${REPO_ROOT}/guide/scripts/run_young_gulp_campaign_shard.qsub.sh"
MANIFEST_PATH="${CAMPAIGN_ROOT}/campaign_manifest.json"
SUBMISSIONS_TSV="${CAMPAIGN_ROOT}/submissions/submitted_jobs.tsv"

mkdir -p \
  "${CAMPAIGN_ROOT}/projections/receipts" \
  "${CAMPAIGN_ROOT}/projections/states" \
  "${CAMPAIGN_ROOT}/health" \
  "${CAMPAIGN_ROOT}/metrics" \
  "${CAMPAIGN_ROOT}/runs" \
  "${CAMPAIGN_ROOT}/shards" \
  "${CAMPAIGN_ROOT}/submissions"

cat > "${MANIFEST_PATH}" <<JSON
{
  "campaign_root": "${CAMPAIGN_ROOT}",
  "repo_root": "${REPO_ROOT}",
  "created_at": "$(date -u +%Y-%m-%dT%H:%M:%SZ)",
  "scheduler_family": "grid_engine",
  "site_name": "young",
  "shard_count": ${SHARD_COUNT},
  "candidates_per_shard": ${CANDIDATES_PER_SHARD},
  "workers_per_shard": ${WORKERS},
  "notes": "first shared-root Young multi-qsub coordination artifact"
}
JSON

printf "shard_id\tjob_id\tcandidate_offset\tcandidate_count\n" > "${SUBMISSIONS_TSV}"

echo "Campaign root: ${CAMPAIGN_ROOT}"
echo "Submitting ${SHARD_COUNT} independent qsub shard jobs..."

for shard_index in $(seq 0 $((SHARD_COUNT - 1))); do
  shard_id="$(printf 'shard-%02d' "${shard_index}")"
  candidate_offset=$((shard_index * CANDIDATES_PER_SHARD))

  cat > "${CAMPAIGN_ROOT}/shards/${shard_id}.json" <<JSON
{
  "shard_id": "${shard_id}",
  "scheduler_receipt_id": null,
  "member_job_ids": ["campaign-shard:${shard_id}"],
  "assignment_policy": "static_membership",
  "max_concurrency": ${WORKERS},
  "retry_policy": {
    "max_attempts_per_job": 2,
    "max_reclaims_per_job": 1,
    "retry_failed_jobs": true
  },
  "telemetry_rollup": {
    "running_jobs": 0,
    "completed_jobs": 0,
    "failed_jobs": 0,
    "peak_rss_mb": null,
    "nodes": []
  },
  "terminal_summary_relpath": "health/${shard_id}.terminal_summary.json"
}
JSON

  submit_output="$(
    qsub \
      -terse \
      -N "patina_${shard_id}" \
      -v REPO_ROOT="${REPO_ROOT}",PATINA_YOUNG_CAMPAIGN_ROOT="${CAMPAIGN_ROOT}",PATINA_SHARD_ID="${shard_id}",PATINA_CANDIDATE_COUNT="${CANDIDATES_PER_SHARD}",PATINA_CANDIDATE_OFFSET="${candidate_offset}",PATINA_WORKERS="${WORKERS}" \
      "${SHARD_SCRIPT}"
  )"
  job_id="${submit_output%%.*}"

  printf "%s\t%s\t%s\t%s\n" "${shard_id}" "${job_id}" "${candidate_offset}" "${CANDIDATES_PER_SHARD}" >> "${SUBMISSIONS_TSV}"
  echo "  ${shard_id} -> ${job_id}"
done

echo
echo "Submitted Young multi-qsub campaign."
echo "Next:"
echo "  python3 \"${REPO_ROOT}/guide/scripts/inspect_young_campaign_health.py\" \"${CAMPAIGN_ROOT}\""
echo "Artifacts:"
echo "  manifest:     ${MANIFEST_PATH}"
echo "  submissions:  ${SUBMISSIONS_TSV}"
