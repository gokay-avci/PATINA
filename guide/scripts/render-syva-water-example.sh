#!/usr/bin/env bash
set -euo pipefail

script_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
guide_dir="$(cd "${script_dir}/.." && pwd)"
repo_root="$(cd "${guide_dir}/.." && pwd)"
xyz_path="${guide_dir}/examples/syva/water.xyz"

printf '### Generated `patina-tools cluster-symmetry` Output\n\n'
printf 'Command run at build time:\n\n'
printf '```bash\n'
printf 'cargo run -q -p patina-tools -- cluster-symmetry --xyz guide/examples/syva/water.xyz --skip-tolerance-scan\n'
printf '```\n\n'
printf 'Result:\n\n'
printf '```json\n'
(
  cd "${repo_root}"
  CARGO_TERM_COLOR=never cargo run -q -p patina-tools -- cluster-symmetry --xyz "${xyz_path}" --skip-tolerance-scan
)
printf '\n```\n'
