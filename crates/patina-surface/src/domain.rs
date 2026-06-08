// Source-to-target mapping for this module:
// - existing `crates/patina-surface/src/lib.rs` public domain contracts
// - future imports from `to_integrate_project/crystal_surface_generator/src/core/structure.rs`
// - future imports from `to_integrate_project/crystal_surface_generator/src/chemistry/*`

use nalgebra::{Matrix3, Vector3};
use patina_raspa::PeriodicFramework;
use patina_sci_kernel::{Framework3D, Slab2D, StructureError, StructureLike};
use patina_types::Candidate;
use serde::{Deserialize, Serialize};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum SurfaceInterfaceError {
    #[error("surface parent conversion requires a lattice")]
    MissingLattice,
    #[error("surface parent conversion requires a 3D periodic framework")]
    NonThreeDimensionalFramework,
    #[error("surface parent conversion currently supports only fully periodic 3D inputs; received periodic_axes={periodic_axes:?}")]
    PartialPeriodicityUnsupported { periodic_axes: [bool; 3] },
    #[error("surface candidate validation failed: {0}")]
    InvalidCandidate(String),
    #[error("framework has no atoms")]
    EmptyFramework,
    #[error("miller index [0 0 0] is invalid for slab generation")]
    ZeroMillerIndex,
    #[error("thickness must be positive, received {0}")]
    InvalidThickness(f64),
    #[error("vacuum must be non-negative, received {0}")]
    InvalidVacuum(f64),
    #[error(
        "surface supercell repeats must be positive, received repeat_a={repeat_a}, repeat_b={repeat_b}"
    )]
    InvalidSupercell { repeat_a: usize, repeat_b: usize },
    #[error("slab dedup fractional tolerance must be positive and finite, received {0}")]
    InvalidDedupTolerance(f64),
    #[error("surface lattice has zero or near-zero volume")]
    DegenerateLattice,
    #[error("surface lattice is not invertible")]
    NonInvertibleLattice,
    #[error("invalid Miller indices produced a near-zero reciprocal normal")]
    DegenerateSurfaceNormal,
    #[error("surface math kernel failed: {0}")]
    MathKernel(String),
    #[error("surface graph kernel failed: {0}")]
    GraphKernel(String),
    #[error("surface generation produced an empty slab")]
    EmptySlab,
    #[error("surface slab conversion requires exactly two periodic axes; received periodic_axes={periodic_axes:?}")]
    NonTwoDimensionalSlabCandidate { periodic_axes: [bool; 3] },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MillerIndex {
    pub h: i32,
    pub k: i32,
    pub l: i32,
}

impl MillerIndex {
    pub fn new(h: i32, k: i32, l: i32) -> Result<Self, SurfaceInterfaceError> {
        if h == 0 && k == 0 && l == 0 {
            return Err(SurfaceInterfaceError::ZeroMillerIndex);
        }
        Ok(Self { h, k, l })
    }

    pub fn as_array(&self) -> [i32; 3] {
        [self.h, self.k, self.l]
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SurfaceCutStrategy {
    FixedOffset,
    TopologyAware,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SurfaceReconstructionMode {
    None,
    IonicBalance,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SurfaceTerminationBias {
    Neutral,
    NodeExposed,
    LinkerExposed,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DedupConfig {
    pub frac_tol: f64,
    pub inplane_only: bool,
    pub require_same_element: bool,
}

impl Default for DedupConfig {
    fn default() -> Self {
        Self {
            frac_tol: 2.0e-4,
            inplane_only: true,
            require_same_element: true,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct SlabReductionConfig {
    pub dedup_slab: bool,
    pub reduce_slab_inplane: bool,
    pub dedup: DedupConfig,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SurfaceSupercellConfig {
    pub repeat_a: usize,
    pub repeat_b: usize,
}

impl Default for SurfaceSupercellConfig {
    fn default() -> Self {
        Self {
            repeat_a: 1,
            repeat_b: 1,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DedupReport {
    pub before_atoms: usize,
    pub after_atoms: usize,
    pub removed: usize,
    pub frac_tol: f64,
    pub inplane_only: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SurfaceGenerationConfig {
    pub miller: MillerIndex,
    pub thickness_angstrom: f64,
    pub vacuum_angstrom: f64,
    #[serde(default)]
    pub supercell: SurfaceSupercellConfig,
    pub cut_strategy: SurfaceCutStrategy,
    pub cut_offset_fraction: Option<f64>,
    pub slab_reduction: SlabReductionConfig,
    pub reconstruction: SurfaceReconstructionMode,
    pub termination_bias: SurfaceTerminationBias,
}

impl SurfaceGenerationConfig {
    pub fn validate(&self) -> Result<(), SurfaceInterfaceError> {
        if self.thickness_angstrom <= 0.0 || !self.thickness_angstrom.is_finite() {
            return Err(SurfaceInterfaceError::InvalidThickness(
                self.thickness_angstrom,
            ));
        }
        if self.vacuum_angstrom < 0.0 || !self.vacuum_angstrom.is_finite() {
            return Err(SurfaceInterfaceError::InvalidVacuum(self.vacuum_angstrom));
        }
        if self.supercell.repeat_a == 0 || self.supercell.repeat_b == 0 {
            return Err(SurfaceInterfaceError::InvalidSupercell {
                repeat_a: self.supercell.repeat_a,
                repeat_b: self.supercell.repeat_b,
            });
        }
        Ok(())
    }
}

impl Default for SurfaceGenerationConfig {
    fn default() -> Self {
        Self {
            miller: MillerIndex { h: 1, k: 0, l: 0 },
            thickness_angstrom: 10.0,
            vacuum_angstrom: 15.0,
            supercell: SurfaceSupercellConfig::default(),
            cut_strategy: SurfaceCutStrategy::TopologyAware,
            cut_offset_fraction: None,
            slab_reduction: SlabReductionConfig::default(),
            reconstruction: SurfaceReconstructionMode::IonicBalance,
            termination_bias: SurfaceTerminationBias::Neutral,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SurfaceFrameworkAtom {
    pub species: String,
    pub fractional: [f64; 3],
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SurfaceParentStructure {
    pub label: String,
    pub lattice: [[f64; 3]; 3],
    pub periodic_axes: [bool; 3],
    pub atoms: Vec<SurfaceFrameworkAtom>,
}

impl SurfaceParentStructure {
    pub fn try_from_candidate(candidate: &Candidate) -> Result<Self, SurfaceInterfaceError> {
        let framework = Framework3D::try_from(candidate).map_err(map_surface_structure_error)?;
        let atoms = framework
            .sites
            .iter()
            .map(|site| SurfaceFrameworkAtom {
                species: site.species.clone(),
                fractional: site.coords,
            })
            .collect::<Vec<_>>();
        if atoms.is_empty() {
            return Err(SurfaceInterfaceError::EmptyFramework);
        }
        Ok(Self {
            label: framework.metadata.label.clone(),
            lattice: framework
                .lattice
                .ok_or(SurfaceInterfaceError::MissingLattice)?
                .basis,
            periodic_axes: framework.periodic_axes().axes,
            atoms,
        })
    }

    pub fn try_from_periodic_framework(
        framework: &PeriodicFramework,
    ) -> Result<Self, SurfaceInterfaceError> {
        Self::try_from_candidate(&framework.to_candidate())
    }

    pub fn to_candidate(&self) -> Candidate {
        Candidate::periodic(
            self.label.clone(),
            self.atoms.iter().map(|atom| atom.species.clone()).collect(),
            self.atoms.iter().map(|atom| atom.fractional).collect(),
            self.lattice,
            self.periodic_axes,
        )
    }

    pub fn try_into_framework3d(&self) -> Result<Framework3D, SurfaceInterfaceError> {
        Framework3D::try_from(&self.to_candidate()).map_err(map_surface_structure_error)
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SurfaceAtom {
    pub species: String,
    pub fractional: [f64; 3],
    pub cartesian: [f64; 3],
    pub source_fractional: Option<[f64; 3]>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SurfaceDiagnosticsDataset {
    pub n_atoms: usize,
    pub n_bonds: usize,
    pub n_components: usize,
    pub largest_component: usize,
    pub element_counts: Vec<(String, usize)>,
    pub element_degrees: Vec<(String, (usize, usize, f64))>,
    pub isolated_atoms: Vec<usize>,
    pub suspicious_atoms: Vec<SurfaceSuspiciousCoordination>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SurfaceSuspiciousCoordination {
    pub atom_index: usize,
    pub species: String,
    pub degree: usize,
    pub expected_range: (usize, usize),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SurfaceBondDiagnosticsSummary {
    pub n_atoms: usize,
    pub z_skin_angstrom: f64,
    pub z_min: f64,
    pub z_max: f64,
    pub bottom_indices: Vec<usize>,
    pub top_indices: Vec<usize>,
    pub dangling_candidates: Vec<SurfaceDanglingBondCandidate>,
    pub surface_stats: Vec<(String, (usize, f64))>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SurfaceDanglingBondCandidate {
    pub atom_index: usize,
    pub species: String,
    pub degree: usize,
    pub expected_min: usize,
    pub region: SurfaceFace,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SurfaceTopologyDiagnostics {
    pub topology_safe_cut: Option<bool>,
    pub broken_bond_estimate: Option<usize>,
    pub dedup_report: Option<DedupReport>,
    pub chosen_cut_offset_angstrom: Option<f64>,
    pub interplanar_spacing_angstrom: Option<f64>,
    pub layer_count: Option<usize>,
    pub graph_diagnostics: Option<SurfaceDiagnosticsDataset>,
    pub surface_bond_summary: Option<SurfaceBondDiagnosticsSummary>,
    pub warnings: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SurfaceSlab {
    pub label: String,
    pub parent_label: String,
    pub miller: MillerIndex,
    pub lattice: [[f64; 3]; 3],
    pub periodic_axes: [bool; 3],
    pub atoms: Vec<SurfaceAtom>,
    pub thickness_angstrom: f64,
    pub vacuum_angstrom: f64,
}

impl SurfaceSlab {
    pub fn try_from_candidate(candidate: &Candidate) -> Result<Self, SurfaceInterfaceError> {
        let slab = Slab2D::try_from(candidate).map_err(map_surface_structure_error)?;
        let lattice = slab
            .lattice
            .ok_or(SurfaceInterfaceError::MissingLattice)?
            .basis;
        let lattice_matrix = array_to_matrix3(lattice);
        let atoms = slab
            .sites
            .iter()
            .map(|site| SurfaceAtom {
                species: site.species.clone(),
                fractional: site.coords,
                cartesian: vector3_to_array(
                    lattice_matrix * Vector3::new(site.coords[0], site.coords[1], site.coords[2]),
                ),
                source_fractional: None,
            })
            .collect();
        Ok(Self {
            label: slab.metadata.label.clone(),
            parent_label: slab.metadata.label.clone(),
            miller: MillerIndex { h: 0, k: 0, l: 0 },
            lattice,
            periodic_axes: slab.periodic_axes.axes,
            atoms,
            thickness_angstrom: 0.0,
            vacuum_angstrom: 0.0,
        })
    }

    pub fn to_candidate(&self) -> Candidate {
        Candidate::periodic(
            self.label.clone(),
            self.atoms.iter().map(|atom| atom.species.clone()).collect(),
            self.atoms.iter().map(|atom| atom.fractional).collect(),
            self.lattice,
            self.periodic_axes,
        )
    }

    pub fn try_into_slab2d(&self) -> Result<Slab2D, SurfaceInterfaceError> {
        Slab2D::try_from(&self.to_candidate()).map_err(map_surface_structure_error)
    }
}

impl From<&SurfaceParentStructure> for Framework3D {
    fn from(value: &SurfaceParentStructure) -> Self {
        value.try_into_framework3d().unwrap_or_else(|error| {
            panic!("validated surface parent structure must map to Framework3D: {error}")
        })
    }
}

impl From<&SurfaceSlab> for Slab2D {
    fn from(value: &SurfaceSlab) -> Self {
        value
            .try_into_slab2d()
            .unwrap_or_else(|error| panic!("validated surface slab must map to Slab2D: {error}"))
    }
}

fn map_surface_structure_error(error: StructureError) -> SurfaceInterfaceError {
    match error {
        StructureError::UnexpectedPeriodicAxes { actual, .. }
            if actual == patina_sci_kernel::PeriodicAxes::NONE =>
        {
            SurfaceInterfaceError::NonThreeDimensionalFramework
        }
        StructureError::UnexpectedPeriodicAxes { actual, .. } => {
            SurfaceInterfaceError::PartialPeriodicityUnsupported {
                periodic_axes: actual.axes,
            }
        }
        StructureError::UnexpectedPeriodicDimensionCount {
            expected: 2,
            periodic_axes,
            ..
        } => SurfaceInterfaceError::NonTwoDimensionalSlabCandidate {
            periodic_axes: periodic_axes.axes,
        },
        StructureError::UnexpectedPeriodicDimensionCount { periodic_axes, .. }
            if periodic_axes.count() == 0 =>
        {
            SurfaceInterfaceError::NonThreeDimensionalFramework
        }
        StructureError::UnexpectedPeriodicDimensionCount { periodic_axes, .. } => {
            SurfaceInterfaceError::PartialPeriodicityUnsupported {
                periodic_axes: periodic_axes.axes,
            }
        }
        StructureError::MissingLatticeForPeriodicAxes { .. } => {
            SurfaceInterfaceError::MissingLattice
        }
        other => SurfaceInterfaceError::InvalidCandidate(format!("{other:?}")),
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SurfaceGenerationRequest {
    pub parent: SurfaceParentStructure,
    pub config: SurfaceGenerationConfig,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SurfaceGenerationResult {
    pub slab: SurfaceSlab,
    pub diagnostics: SurfaceTopologyDiagnostics,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SurfacePolarityClass {
    NonPolar,
    Polar,
    Indeterminate,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SurfaceFace {
    Top,
    Bottom,
    Both,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DipoleCancellationPolicy {
    None,
    FullCancellation,
    BestEffort,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum IonicMoveKind {
    Vacancy,
    Adatom,
    LayerTransfer,
    Hop,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SurfacePolarityReport {
    pub classification: SurfacePolarityClass,
    pub residual_dipole_proxy_z: Option<f64>,
    pub top_species_counts: Vec<(String, usize)>,
    pub bottom_species_counts: Vec<(String, usize)>,
    pub warnings: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct IonicReconstructionConfig {
    pub target_face: SurfaceFace,
    pub dipole_policy: DipoleCancellationPolicy,
    pub candidate_limit: usize,
    pub max_modified_sites: usize,
    pub maintain_stoichiometry: bool,
    pub supercell: [usize; 2],
    pub frozen_layer_count: usize,
}

impl Default for IonicReconstructionConfig {
    fn default() -> Self {
        Self {
            target_face: SurfaceFace::Both,
            dipole_policy: DipoleCancellationPolicy::BestEffort,
            candidate_limit: 32,
            max_modified_sites: 4,
            maintain_stoichiometry: true,
            supercell: [1, 1],
            frozen_layer_count: 0,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct IonicSiteModification {
    pub kind: IonicMoveKind,
    pub species: String,
    pub face: SurfaceFace,
    pub source_atom_index: Option<usize>,
    pub target_fractional: Option<[f64; 3]>,
    pub notes: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct IonicReconstructionCandidate {
    pub slab: SurfaceSlab,
    pub modifications: Vec<IonicSiteModification>,
    pub polarity: Option<SurfacePolarityReport>,
    pub warnings: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct IonicReconstructionRequest {
    pub slab: SurfaceSlab,
    pub config: IonicReconstructionConfig,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct IonicReconstructionResult {
    pub input_slab: SurfaceSlab,
    pub initial_polarity: Option<SurfacePolarityReport>,
    pub candidates: Vec<IonicReconstructionCandidate>,
    pub warnings: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SurfaceEvaluationBackendKind {
    Gulp,
    Other,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SurfaceOptimizerKind {
    Bfgs,
    Other,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SurfaceRelaxationProtocol {
    pub backend: SurfaceEvaluationBackendKind,
    pub optimizer: SurfaceOptimizerKind,
    pub potential_family: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TwoRegionSurfaceModel {
    pub total_layers: usize,
    pub relaxed_layers: usize,
    pub fixed_layers: usize,
    pub compensated_bottom_layer: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ExternalPointChargeCompensation {
    pub enabled: bool,
    pub point_count: usize,
    pub distance_above_angstrom: f64,
    pub distance_below_angstrom: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SurfaceSiteSamplingRules {
    pub top_layer_positions_only: bool,
    pub forbid_anion_cation_swaps: bool,
    pub bulk_lattice_positions_only: bool,
    pub zn_o_pair_transfer_only: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct StochasticSamplingProtocol {
    pub supercell: [usize; 2],
    pub occupancy_site_count: usize,
    pub occupancy_count: usize,
    pub samples_per_occupancy: usize,
    pub independent_repeats: usize,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CanonicalEnsembleProtocol {
    pub enabled: bool,
    pub occupancy_site_count: usize,
    pub samples_per_occupancy: usize,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GrandCanonicalEnsembleProtocol {
    pub enabled: bool,
    pub occupancy_site_count: usize,
    pub mu_search_discrete_step: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PaperLikeSurfaceProtocol {
    pub protocol_name: String,
    pub material_label: String,
    pub bulk_lattice_parameters_angstrom: Option<[f64; 3]>,
    pub bulk_internal_parameter_u: Option<f64>,
    pub target_surface: MillerIndex,
    pub one_sided_two_d_periodic_model: bool,
    pub region_model: TwoRegionSurfaceModel,
    pub reconstruction: IonicReconstructionConfig,
    pub surface_site_rules: SurfaceSiteSamplingRules,
    pub compensating_charge_grid: ExternalPointChargeCompensation,
    pub relaxation: SurfaceRelaxationProtocol,
    pub sampling: StochasticSamplingProtocol,
    pub canonical_ensemble: CanonicalEnsembleProtocol,
    pub grand_canonical_ensemble: GrandCanonicalEnsembleProtocol,
}

impl PaperLikeSurfaceProtocol {
    pub fn zno_polar_0001_paper_2017() -> Self {
        Self {
            protocol_name: "zno_polar_0001_mora_fonz_2017_like".into(),
            material_label: "ZnO".into(),
            bulk_lattice_parameters_angstrom: Some([3.2518, 3.2518, 5.1969]),
            bulk_internal_parameter_u: Some(0.3806),
            target_surface: MillerIndex { h: 0, k: 0, l: 1 },
            one_sided_two_d_periodic_model: true,
            region_model: TwoRegionSurfaceModel {
                total_layers: 6,
                relaxed_layers: 3,
                fixed_layers: 3,
                compensated_bottom_layer: true,
            },
            reconstruction: IonicReconstructionConfig {
                target_face: SurfaceFace::Top,
                dipole_policy: DipoleCancellationPolicy::FullCancellation,
                candidate_limit: 10_000,
                max_modified_sites: 25,
                maintain_stoichiometry: false,
                supercell: [5, 5],
                frozen_layer_count: 3,
            },
            surface_site_rules: SurfaceSiteSamplingRules {
                top_layer_positions_only: true,
                forbid_anion_cation_swaps: true,
                bulk_lattice_positions_only: true,
                zn_o_pair_transfer_only: false,
            },
            compensating_charge_grid: ExternalPointChargeCompensation {
                enabled: true,
                point_count: 100,
                distance_above_angstrom: 50.0,
                distance_below_angstrom: 50.0,
            },
            relaxation: SurfaceRelaxationProtocol {
                backend: SurfaceEvaluationBackendKind::Gulp,
                optimizer: SurfaceOptimizerKind::Bfgs,
                potential_family: "Whitmore-Sokol-Catlow Born shell ZnO".into(),
            },
            sampling: StochasticSamplingProtocol {
                supercell: [5, 5],
                occupancy_site_count: 25,
                occupancy_count: 26,
                samples_per_occupancy: 10_000,
                independent_repeats: 1,
            },
            canonical_ensemble: CanonicalEnsembleProtocol {
                enabled: true,
                occupancy_site_count: 25,
                samples_per_occupancy: 10_000,
            },
            grand_canonical_ensemble: GrandCanonicalEnsembleProtocol {
                enabled: true,
                occupancy_site_count: 25,
                mu_search_discrete_step: true,
            },
        }
    }
}

pub trait SurfaceGenerationEngine {
    fn generate_surface(
        &self,
        request: &SurfaceGenerationRequest,
    ) -> Result<SurfaceGenerationResult, SurfaceInterfaceError>;
}

pub trait SurfacePolarityAnalyzer {
    fn analyze_surface_polarity(
        &self,
        slab: &SurfaceSlab,
    ) -> Result<SurfacePolarityReport, SurfaceInterfaceError>;
}

pub trait IonicReconstructionEngine {
    fn generate_ionic_reconstructions(
        &self,
        request: &IonicReconstructionRequest,
    ) -> Result<IonicReconstructionResult, SurfaceInterfaceError>;
}

pub(crate) fn array_to_matrix3(value: [[f64; 3]; 3]) -> Matrix3<f64> {
    Matrix3::from_columns(&[
        Vector3::new(value[0][0], value[0][1], value[0][2]),
        Vector3::new(value[1][0], value[1][1], value[1][2]),
        Vector3::new(value[2][0], value[2][1], value[2][2]),
    ])
}

pub(crate) fn matrix3_to_array(value: Matrix3<f64>) -> [[f64; 3]; 3] {
    [
        [value[(0, 0)], value[(1, 0)], value[(2, 0)]],
        [value[(0, 1)], value[(1, 1)], value[(2, 1)]],
        [value[(0, 2)], value[(1, 2)], value[(2, 2)]],
    ]
}

pub(crate) fn vector3_to_array(value: Vector3<f64>) -> [f64; 3] {
    [value.x, value.y, value.z]
}
