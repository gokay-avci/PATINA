// Source-to-target mapping for this module:
// - existing `crates/patina-surface/src/lib.rs` scientific kernels and heuristics
// - future imports from `to_integrate_project/crystal_surface_generator/src/synthesis/*`
// - future imports from `to_integrate_project/crystal_surface_generator/src/analysis/topology.rs`

use crate::analysis::diagnostics::run_diagnostics_with_config;
use crate::analysis::surface_bonds::{analyse_surface_bonds_with_config, SurfaceBondReport};
use crate::cut::find_safe_offsets;
use crate::domain::{
    array_to_matrix3, matrix3_to_array, vector3_to_array, SurfaceAtom, SurfaceCutStrategy,
    SurfaceGenerationConfig, SurfaceGenerationEngine, SurfaceGenerationRequest,
    SurfaceGenerationResult, SurfaceInterfaceError, SurfaceParentStructure,
    SurfacePolarityAnalyzer, SurfacePolarityClass, SurfacePolarityReport,
    SurfaceReconstructionMode, SurfaceSlab, SurfaceTopologyDiagnostics,
};
use crate::generation::{compute_geometry, populate_slab_atoms, LatticeOps, SlabGeometry};
use crate::graph::bonding::BondingConfig;
use crate::reduction::apply_slab_reduction;
use nalgebra::Vector3;

#[derive(Debug, Clone, Default)]
pub struct HeuristicSurfacePolarityAnalyzer;

#[derive(Debug, Clone, Default)]
pub struct DefaultSurfaceGenerationEngine;

impl SurfaceGenerationEngine for DefaultSurfaceGenerationEngine {
    fn generate_surface(
        &self,
        request: &SurfaceGenerationRequest,
    ) -> Result<SurfaceGenerationResult, SurfaceInterfaceError> {
        request.config.validate()?;
        let lattice = LatticeOps::new(request.parent.lattice)?;
        let mut warnings = Vec::new();
        let geometry = compute_geometry(&lattice, &request.config, &mut warnings)?;
        let cut = choose_cut_offset(&request.parent, &lattice, &geometry, &request.config);
        let slab_atoms =
            populate_slab_atoms(&request.parent, &lattice, &geometry, cut.offset_angstrom)?;
        let mut slab = SurfaceSlab {
            label: format!(
                "{}__surface_{}{}{}",
                request.parent.label,
                request.config.miller.h,
                request.config.miller.k,
                request.config.miller.l
            ),
            parent_label: request.parent.label.clone(),
            miller: request.config.miller.clone(),
            lattice: matrix3_to_array(geometry.basis),
            periodic_axes: [true, true, false],
            atoms: slab_atoms,
            thickness_angstrom: request.config.thickness_angstrom,
            vacuum_angstrom: request.config.vacuum_angstrom,
        };
        apply_surface_supercell(
            &mut slab,
            request.config.supercell.repeat_a,
            request.config.supercell.repeat_b,
        );
        if slab.atoms.is_empty() {
            return Err(SurfaceInterfaceError::EmptySlab);
        }
        let dedup_report = apply_slab_reduction(&mut slab, &request.config)?;
        if let Some(report) = &dedup_report {
            warnings.push(format!(
                "slab reduction removed {} duplicate atoms ({} -> {})",
                report.removed, report.before_atoms, report.after_atoms
            ));
        }
        if !matches!(
            request.config.reconstruction,
            SurfaceReconstructionMode::None
        ) {
            warnings.push(
                "surface reconstruction is not yet implemented in patina-surface; emitting unreconstructed slab"
                    .into(),
            );
        }
        let bond_diagnostics = analyze_surface_bond_diagnostics(&slab)?;
        let graph_diagnostics = run_diagnostics_with_config(&slab, BondingConfig::default())?;
        let surface_atom_count =
            bond_diagnostics.bottom_indices.len() + bond_diagnostics.top_indices.len();
        if !bond_diagnostics.dangling_candidates.is_empty() {
            warnings.push(format!(
                "surface bond diagnostics flagged {} low-coordination surface atoms across {} face atoms",
                bond_diagnostics.dangling_candidates.len(),
                surface_atom_count
            ));
        }
        Ok(SurfaceGenerationResult {
            slab,
            diagnostics: SurfaceTopologyDiagnostics {
                topology_safe_cut: cut.topology_safe_cut,
                broken_bond_estimate: Some(bond_diagnostics.dangling_candidates.len()),
                dedup_report,
                chosen_cut_offset_angstrom: Some(cut.offset_angstrom),
                interplanar_spacing_angstrom: Some(geometry.d_hkl),
                layer_count: Some(geometry.n_layers),
                graph_diagnostics: Some(graph_diagnostics.to_surface_dataset()),
                surface_bond_summary: Some(bond_diagnostics.to_summary()),
                warnings,
            },
        })
    }
}

fn apply_surface_supercell(slab: &mut SurfaceSlab, repeat_a: usize, repeat_b: usize) {
    if repeat_a == 1 && repeat_b == 1 {
        return;
    }

    let lattice = array_to_matrix3(slab.lattice);
    let a = lattice.column(0).into_owned();
    let b = lattice.column(1).into_owned();
    let c = lattice.column(2).into_owned();
    let tiled_lattice =
        nalgebra::Matrix3::from_columns(&[a * repeat_a as f64, b * repeat_b as f64, c]);

    let mut tiled_atoms = Vec::with_capacity(slab.atoms.len() * repeat_a * repeat_b);
    for ia in 0..repeat_a {
        for ib in 0..repeat_b {
            for atom in &slab.atoms {
                let new_fractional = [
                    (atom.fractional[0] + ia as f64) / repeat_a as f64,
                    (atom.fractional[1] + ib as f64) / repeat_b as f64,
                    atom.fractional[2],
                ];
                let cartesian = tiled_lattice
                    * Vector3::new(new_fractional[0], new_fractional[1], new_fractional[2]);
                tiled_atoms.push(SurfaceAtom {
                    species: atom.species.clone(),
                    fractional: new_fractional,
                    cartesian: vector3_to_array(cartesian),
                    source_fractional: atom.source_fractional,
                });
            }
        }
    }

    slab.lattice = matrix3_to_array(tiled_lattice);
    slab.atoms = tiled_atoms;
    slab.label = format!("{}__{}x{}", slab.label, repeat_a, repeat_b);
}

impl SurfacePolarityAnalyzer for HeuristicSurfacePolarityAnalyzer {
    fn analyze_surface_polarity(
        &self,
        slab: &SurfaceSlab,
    ) -> Result<SurfacePolarityReport, SurfaceInterfaceError> {
        if slab.atoms.is_empty() {
            return Ok(SurfacePolarityReport {
                classification: SurfacePolarityClass::Indeterminate,
                residual_dipole_proxy_z: None,
                top_species_counts: Vec::new(),
                bottom_species_counts: Vec::new(),
                warnings: vec!["empty slab".into()],
            });
        }

        let lattice = LatticeOps::new(slab.lattice)?;
        let nonperiodic_axis = slab.periodic_axes.iter().position(|axis| !axis).ok_or(
            SurfaceInterfaceError::NonTwoDimensionalSlabCandidate {
                periodic_axes: slab.periodic_axes,
            },
        )?;
        let normal = lattice
            .matrix
            .column(nonperiodic_axis)
            .into_owned()
            .try_normalize(1.0e-15)
            .ok_or(SurfaceInterfaceError::DegenerateSurfaceNormal)?;

        let z_values = slab
            .atoms
            .iter()
            .map(|atom| {
                Vector3::new(atom.cartesian[0], atom.cartesian[1], atom.cartesian[2]).dot(&normal)
            })
            .collect::<Vec<_>>();
        let min_z = z_values.iter().copied().fold(f64::INFINITY, f64::min);
        let max_z = z_values.iter().copied().fold(f64::NEG_INFINITY, f64::max);
        let thickness = (max_z - min_z).max(1.0e-9);
        let face_window = (0.15 * thickness).max(1.0);
        let z_center = 0.5 * (min_z + max_z);

        let mut top_counts = std::collections::BTreeMap::<String, usize>::new();
        let mut bottom_counts = std::collections::BTreeMap::<String, usize>::new();
        let mut warnings = Vec::new();
        let mut residual_dipole_proxy_z = 0.0_f64;
        let mut known_charge_count = 0usize;

        for (atom, z) in slab.atoms.iter().zip(z_values.iter().copied()) {
            if z >= max_z - face_window {
                *top_counts.entry(atom.species.clone()).or_default() += 1;
            }
            if z <= min_z + face_window {
                *bottom_counts.entry(atom.species.clone()).or_default() += 1;
            }
            if let Some(charge) = formal_charge_guess(&atom.species) {
                residual_dipole_proxy_z += charge * (z - z_center);
                known_charge_count += 1;
            }
        }

        if known_charge_count == 0 {
            warnings.push("no formal charge guesses available; polarity is indeterminate".into());
        } else if known_charge_count < slab.atoms.len() {
            warnings.push("formal charge guesses were incomplete; polarity is heuristic".into());
        }

        let classification = if known_charge_count == 0 {
            SurfacePolarityClass::Indeterminate
        } else if residual_dipole_proxy_z.abs() > 1.0e-3 {
            SurfacePolarityClass::Polar
        } else {
            SurfacePolarityClass::NonPolar
        };

        Ok(SurfacePolarityReport {
            classification,
            residual_dipole_proxy_z: Some(residual_dipole_proxy_z),
            top_species_counts: top_counts.into_iter().collect(),
            bottom_species_counts: bottom_counts.into_iter().collect(),
            warnings,
        })
    }
}

#[derive(Debug, Clone)]
struct CutSelection {
    offset_angstrom: f64,
    topology_safe_cut: Option<bool>,
}

fn choose_cut_offset(
    parent: &SurfaceParentStructure,
    lattice: &LatticeOps,
    geometry: &SlabGeometry,
    config: &SurfaceGenerationConfig,
) -> CutSelection {
    match config.cut_strategy {
        SurfaceCutStrategy::FixedOffset => CutSelection {
            offset_angstrom: config.cut_offset_fraction.unwrap_or(0.0) * geometry.d_hkl,
            topology_safe_cut: None,
        },
        SurfaceCutStrategy::TopologyAware => {
            if let Some(fraction) = config.cut_offset_fraction {
                return CutSelection {
                    offset_angstrom: fraction * geometry.d_hkl,
                    topology_safe_cut: None,
                };
            }
            let cuts = find_safe_offsets(parent, lattice, geometry.slab_normal);
            if let Some(best) = cuts.first() {
                CutSelection {
                    offset_angstrom: best.offset_angstrom,
                    topology_safe_cut: Some(best.gap_size > 0.5),
                }
            } else {
                CutSelection {
                    offset_angstrom: 0.0,
                    topology_safe_cut: Some(false),
                }
            }
        }
    }
}

pub(crate) fn analyze_surface_bond_diagnostics(
    slab: &SurfaceSlab,
) -> Result<SurfaceBondReport, SurfaceInterfaceError> {
    analyse_surface_bonds_with_config(slab, BondingConfig::default(), 3.0)
}

fn formal_charge_guess(species: &str) -> Option<f64> {
    match species.trim() {
        "H" => Some(1.0),
        "Li" | "Na" | "K" | "Rb" | "Cs" => Some(1.0),
        "Mg" | "Ca" | "Sr" | "Ba" | "Zn" | "Cd" => Some(2.0),
        "Al" => Some(3.0),
        "Si" | "Ti" => Some(4.0),
        "O" | "S" | "Se" | "Te" => Some(-2.0),
        "F" | "Cl" | "Br" | "I" => Some(-1.0),
        _ => None,
    }
}
