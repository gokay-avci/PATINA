use crate::domain::{vector3_to_array, SurfaceAtom, SurfaceInterfaceError, SurfaceParentStructure};
use crate::generation::builder::{LatticeOps, SlabGeometry};
use nalgebra::Vector3;

pub(crate) fn populate_slab_atoms(
    parent: &SurfaceParentStructure,
    lattice: &LatticeOps,
    geometry: &SlabGeometry,
    offset_angstrom: f64,
) -> Result<Vec<SurfaceAtom>, SurfaceInterfaceError> {
    let slab_c = geometry.basis.column(2).into_owned();
    let slab_normal = slab_c
        .try_normalize(1.0e-15)
        .ok_or(SurfaceInterfaceError::DegenerateSurfaceNormal)?;

    let offset_idx = offset_angstrom / geometry.d_hkl;
    let n_layers_float = geometry.n_layers as f64;
    let epsilon = 1.0e-3;
    let min_idx = offset_idx - epsilon;
    let max_idx = offset_idx + n_layers_float - epsilon;

    let a = lattice.matrix.column(0).into_owned();
    let b = lattice.matrix.column(1).into_owned();
    let c = lattice.matrix.column(2).into_owned();

    let p_a = a.dot(&slab_normal).abs();
    let p_b = b.dot(&slab_normal).abs();
    let p_c = c.dot(&slab_normal).abs();
    let total_slab_height = geometry.n_layers as f64 * geometry.d_hkl;
    let margin = 2.0 * geometry.d_hkl + 1.0e-6;
    let eps = 1.0e-12;
    let pad = 2_i32;
    let clamp = |x: i32| x.clamp(0, 4096);
    let na = clamp(if p_a < 1.0e-6 {
        pad
    } else {
        ((total_slab_height + margin) / p_a.max(eps)).ceil() as i32 + pad
    });
    let nb = clamp(if p_b < 1.0e-6 {
        pad
    } else {
        ((total_slab_height + margin) / p_b.max(eps)).ceil() as i32 + pad
    });
    let nc = clamp(if p_c < 1.0e-6 {
        pad
    } else {
        ((total_slab_height + margin) / p_c.max(eps)).ceil() as i32 + pad
    });

    let mut final_atoms = Vec::<(String, Vector3<f64>, [f64; 3])>::new();
    for i in -na..=na {
        for j in -nb..=nb {
            for k in -nc..=nc {
                let shift_frac = [i as f64, j as f64, k as f64];
                let cell_shift_cart = lattice.to_cartesian(shift_frac);
                for atom in &parent.atoms {
                    let pos_cart = lattice.to_cartesian(atom.fractional) + cell_shift_cart;
                    let z_ang = pos_cart.dot(&slab_normal);
                    let layer_val = z_ang / geometry.d_hkl;
                    if layer_val >= min_idx && layer_val < max_idx {
                        final_atoms.push((atom.species.clone(), pos_cart, atom.fractional));
                    }
                }
            }
        }
    }

    if final_atoms.is_empty() {
        return Err(SurfaceInterfaceError::EmptySlab);
    }

    let mut min_z = f64::INFINITY;
    let mut max_z = f64::NEG_INFINITY;
    for (_, pos, _) in &final_atoms {
        let z = pos.dot(&slab_normal);
        min_z = min_z.min(z);
        max_z = max_z.max(z);
    }
    let current_material_thickness = max_z - min_z;
    let total_box_height = slab_c.norm();
    let target_z_start = (total_box_height - current_material_thickness) / 2.0;
    let shift_vec = slab_normal * (target_z_start - min_z);

    let lu = geometry.basis.lu();
    Ok(final_atoms
        .into_iter()
        .map(|(species, pos, source_fractional)| {
            let shifted_cart = pos + shift_vec;
            let fractional = lu
                .solve(&shifted_cart)
                .or_else(|| geometry.basis.try_inverse().map(|inv| inv * shifted_cart))
                .unwrap_or(shifted_cart);
            SurfaceAtom {
                species,
                fractional: vector3_to_array(fractional),
                cartesian: vector3_to_array(shifted_cart),
                source_fractional: Some(source_fractional),
            }
        })
        .collect())
}
