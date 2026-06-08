// Source-to-target mapping for this module:
// - `to_integrate_project/crystal_surface_generator/src/synthesis/reduction.rs`
// - existing slab dedup/reduction logic previously embedded in `engine.rs`

use crate::domain::{
    vector3_to_array, DedupConfig, DedupReport, SurfaceAtom, SurfaceInterfaceError, SurfaceSlab,
};
use crate::generation::LatticeOps;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
struct DedupKey {
    ix: i64,
    iy: i64,
    iz: i64,
}

pub(crate) fn dedup_atoms_inplace(
    slab: &mut SurfaceSlab,
    config: &DedupConfig,
) -> Result<DedupReport, SurfaceInterfaceError> {
    if config.frac_tol <= 0.0 || !config.frac_tol.is_finite() {
        return Err(SurfaceInterfaceError::InvalidDedupTolerance(
            config.frac_tol,
        ));
    }

    let before_atoms = slab.atoms.len();
    if before_atoms <= 1 {
        return Ok(DedupReport {
            before_atoms,
            after_atoms: before_atoms,
            removed: 0,
            frac_tol: config.frac_tol,
            inplane_only: config.inplane_only,
        });
    }

    let lattice = LatticeOps::new(slab.lattice)?;
    let mut grid =
        std::collections::HashMap::<DedupKey, Vec<usize>>::with_capacity(before_atoms * 2);
    let mut kept_atoms = Vec::<SurfaceAtom>::with_capacity(before_atoms);

    for atom in slab.atoms.drain(..) {
        let normalized_fractional = normalize_fractional(atom.fractional, config.inplane_only);
        let key = quantize_dedup_key(normalized_fractional, config.frac_tol);

        let mut duplicate = false;
        for neighbor in dedup_neighbor_keys(key) {
            if let Some(candidates) = grid.get(&neighbor) {
                for &index in candidates {
                    let other = &kept_atoms[index];
                    if config.require_same_element && other.species != atom.species {
                        continue;
                    }
                    if fractional_close(
                        normalized_fractional,
                        other.fractional,
                        config.frac_tol,
                        config.inplane_only,
                    ) {
                        duplicate = true;
                        break;
                    }
                }
            }
            if duplicate {
                break;
            }
        }

        if duplicate {
            continue;
        }

        let kept_index = kept_atoms.len();
        kept_atoms.push(SurfaceAtom {
            species: atom.species,
            fractional: normalized_fractional,
            cartesian: vector3_to_array(lattice.to_cartesian(normalized_fractional)),
            source_fractional: atom.source_fractional,
        });
        grid.entry(key).or_default().push(kept_index);
    }

    slab.atoms = kept_atoms;
    let after_atoms = slab.atoms.len();
    Ok(DedupReport {
        before_atoms,
        after_atoms,
        removed: before_atoms.saturating_sub(after_atoms),
        frac_tol: config.frac_tol,
        inplane_only: config.inplane_only,
    })
}

fn normalize_fractional(fractional: [f64; 3], inplane_only: bool) -> [f64; 3] {
    let mut normalized = fractional;
    normalized[0] = normalized[0].rem_euclid(1.0);
    normalized[1] = normalized[1].rem_euclid(1.0);
    if !inplane_only {
        normalized[2] = normalized[2].rem_euclid(1.0);
    }
    normalized
}

fn quantize_dedup_key(fractional: [f64; 3], tolerance: f64) -> DedupKey {
    let scale = 1.0 / tolerance;
    DedupKey {
        ix: (fractional[0] * scale).floor() as i64,
        iy: (fractional[1] * scale).floor() as i64,
        iz: (fractional[2] * scale).floor() as i64,
    }
}

fn dedup_neighbor_keys(key: DedupKey) -> [DedupKey; 27] {
    let mut neighbors = [DedupKey {
        ix: 0,
        iy: 0,
        iz: 0,
    }; 27];
    let mut index = 0usize;
    for dx in -1..=1 {
        for dy in -1..=1 {
            for dz in -1..=1 {
                neighbors[index] = DedupKey {
                    ix: key.ix + dx,
                    iy: key.iy + dy,
                    iz: key.iz + dz,
                };
                index += 1;
            }
        }
    }
    neighbors
}

fn fractional_close(left: [f64; 3], right: [f64; 3], tolerance: f64, inplane_only: bool) -> bool {
    let mut dx = (left[0] - right[0]).abs();
    let mut dy = (left[1] - right[1]).abs();
    dx = dx.min(1.0 - dx);
    dy = dy.min(1.0 - dy);
    if dx > tolerance || dy > tolerance {
        return false;
    }
    let dz = if inplane_only {
        (left[2] - right[2]).abs()
    } else {
        let raw = (left[2] - right[2]).abs();
        raw.min(1.0 - raw)
    };
    dz <= tolerance
}
