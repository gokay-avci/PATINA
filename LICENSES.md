# Repository License Notes

This repository now uses the MIT license for original repository-authored code, with the standard
license text at [`LICENSE`](./LICENSE).

That does not mean every file in the repository can already be described as provenance-free MIT
code.

## Default Rule

Unless a file or subtree says otherwise, repository-authored code and documentation should be
treated as MIT-licensed.

## Known Exception Lanes

### `crates/patina-perturber` Fingerprint2 provenance lane

The `patina-perturber` environment-fingerprint work contains explicit provenance markers tying parts
of the implementation and parity fixtures to the `Fingerprint2` reference package.

Relevant local provenance records:

- [docs/FINGERPRINT2_LICENSE_AND_PROVENANCE_POLICY_2026-04-16.md](./docs/FINGERPRINT2_LICENSE_AND_PROVENANCE_POLICY_2026-04-16.md)
- [docs/FINGERPRINT2_PERTURBER_CAMPAIGN_2026-04-16.md](./docs/FINGERPRINT2_PERTURBER_CAMPAIGN_2026-04-16.md)
- [crates/patina-perturber/src/environment_fingerprint.rs](./crates/patina-perturber/src/environment_fingerprint.rs)
- [crates/patina-perturber/src/assignment_distance.rs](./crates/patina-perturber/src/assignment_distance.rs)

That lane needs a stricter publish-facing review before it should be described casually as a clean
standalone MIT crate.

### `crates/patina-dreadnaut/nauty25r9`

This subtree contains vendored upstream source and should be treated according to its upstream terms
and notices, not automatically relicensed by the repository MIT file.

Relevant local references:

- [crates/patina-dreadnaut/nauty25r9/README](./crates/patina-dreadnaut/nauty25r9/README)

## Publication Guidance

- Do not assume a root MIT file is enough to mark every workspace crate as `license = "MIT"`.
- Add crate manifest license metadata only after crate-by-crate provenance review.
- Prefer `publish = false` for crates with unresolved third-party, vendored, or provenance-sensitive
  boundaries.
