# Fingerprint2 Fixture Provenance

These fixtures were generated locally on 2026-04-16 from a reference package checkout:

- `Fingerprint2`

Reference inputs copied here:

- `benzene.xyz`
- `lial_hydrate.ascii`
- `perovskite.ascii`

Reference outputs copied here:

- `benzene_fingerprint.dat`
- `lial_hydrate_fingerprint.dat`
- `perovskite_assignment_distance.txt`

Generation process used:

1. Compile `Fingerprint2/src/precision.f90`, `hung.f90`, and `fingerprint.f90` with `gfortran`.
2. Link and run:
   - `Fingerprint2/src/fp_for_clusters.f90`
   - `Fingerprint2/src/fp_for_crystals.f90`
   - `Fingerprint2/src/fp_distance.f90`
3. Capture `build/fingerprint.dat` after each run.
4. Capture the printed assignment distance for `perovskite.ascii`:
   - `FIGNERPRINT DISTANCE OF CONFIGURATION 01 AND 02:    5.33022`

These fixtures are used only for parity testing of the new environment-fingerprint and
assignment-distance lanes in `crates/patina-perturber`.
