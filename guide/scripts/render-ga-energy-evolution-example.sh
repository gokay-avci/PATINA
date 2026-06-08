#!/usr/bin/env bash
set -euo pipefail

script_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
guide_dir="$(cd "${script_dir}/.." && pwd)"
repo_root="$(cd "${guide_dir}/.." && pwd)"
run_dir="${guide_dir}/examples/ga/toy_run"

printf '### Generated `patina-tools energy-evolution` Output\n\n'
printf 'Command run at build time:\n\n'
printf '```bash\n'
printf 'cargo run -q -p patina-tools -- energy-evolution --run-dir guide/examples/ga/toy_run\n'
printf '```\n\n'
printf 'Result:\n\n'
printf '```json\n'
(
  cd "${repo_root}"
  CARGO_TERM_COLOR=never cargo run -q -p patina-tools -- energy-evolution --run-dir "${run_dir}"
)
printf '\n```\n'
