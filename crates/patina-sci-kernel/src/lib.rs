#![forbid(unsafe_code)]

/*!
Pure scientific-kernel types for shared structure semantics and conversion boundaries.

This crate is intentionally small in its first milestone:

- `structure` owns the core scientific vocabulary and validated structure wrappers
- `transform` reserves the landing zone for pure structure-to-structure transforms
- `conversion` owns conversion reporting and policy scaffolding
- `bridge` provides opt-in compatibility with `patina-types::Candidate`

The current implementation is a Phase 0 scaffold. It is meant to compile cleanly and establish the
crate boundary before larger migrations begin.
*/

pub mod analysis;
pub mod codec;
pub mod conversion;
pub mod geometry;
pub mod structure;
pub mod topology;
pub mod transform;

#[cfg(feature = "bridge-patina-types")]
pub mod bridge;

pub use analysis::{
    atomic_mass, center_of_mass_cartesian, inertia_tensor_cartesian, PrincipalMomentAnalysis,
};
#[cfg(feature = "bridge-patina-types")]
pub use analysis::{
    candidate_cartesian_positions, candidate_center_of_mass, candidate_inertia_tensor,
    candidate_principal_moment_analysis,
};
#[cfg(feature = "bridge-patina-types")]
pub use codec::cif::{
    candidate_from_cif_block, candidate_from_cif_block_for_scientific_use,
    candidate_from_cif_block_with_policy, candidate_from_cif_path,
    candidate_from_cif_path_with_policy, CifBridgeError,
};
pub use codec::cif::{
    parse_cif_str, parse_cif_symmetry_operation, read_cif_path, validate_cif_block,
    validate_cif_for_scientific_use, validate_cif_geometry, CifAtomSite, CifCell, CifDataBlock,
    CifDialect, CifError, CifExpandedSite, CifFinding, CifFindingCode, CifFindingSeverity,
    CifGeometryPolicy, CifRadiusMode, CifScientificPolicy, CifSpeciesRadius, CifSymmetryOperation,
    CifValidationPolicy,
};
pub use codec::xyz::{
    encode_xyz_frames, parse_extxyz_lattice, parse_extxyz_pbc, parse_xyz_frames, write_xyz_frames,
    AtomRecord, XyzCoordinateMode, XyzEncodeOptions, XyzError, XyzFrame,
};
pub use conversion::{
    ConversionError, ConversionKind, ConversionOutcome, ConversionPolicy, ConversionWarning,
    InferenceKind, LossKind,
};
pub use geometry::{
    assess_periodic_cell, cartesian_to_fractional, fractional_to_cartesian,
    lattice_params_from_vectors, lattice_vectors_from_params, minimum_image_cartesian_distance_sq,
    minimum_image_cartesian_distance_sq_with_axes, nearest_periodic_image_fractional,
    nearest_periodic_image_fractional_with_axes, normalize_fractional_coordinate,
    normalize_fractional_coordinate_with_axes, CellAssessment, GeometryRejectionReason,
    UnitCellParameters,
};
pub use structure::{
    Cluster0D, CoordinateBasis, Framework3D, Lattice3, PeriodicAxes, RawStructure, Site, Slab2D,
    StructureDimensionality, StructureError, StructureLike, StructureMetadata, StructureType,
    Wire1D,
};
pub use topology::{
    build_structure_edges, classify_topology_verdict, compute_structure_hashkey_radius,
    summarize_coordination_histograms, summarize_edge_pairs, AtomSpec, HashkeyRadiusMode,
    TopologyAtom, TopologyError,
};
pub use transform::TransformError;
