use serde::Serialize;

use super::rust_janus_ga::RustJanusDuplicatePolicyMode;

#[derive(Debug, Clone, Copy, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum FilterKind {
    BinaryIdentity,
    ThresholdDuplicate,
}

#[derive(Debug, Clone, Serialize)]
pub struct FilterDescriptor {
    pub name: &'static str,
    pub kind: FilterKind,
    pub deterministic: bool,
    pub output_mode: &'static str,
    pub active: bool,
    pub notes: &'static str,
}

pub fn rust_janus_filter_stack(mode: RustJanusDuplicatePolicyMode) -> Vec<FilterDescriptor> {
    let identity = match mode {
        RustJanusDuplicatePolicyMode::ExternalNativeHashkey => FilterDescriptor {
            name: "scott_native_hashkey",
            kind: FilterKind::BinaryIdentity,
            deterministic: true,
            output_mode: "binary",
            active: true,
            notes: "Canonical native-style identity filter via SCOTT-compatible graph generation and dreadnaut canonical labelling.",
        },
        RustJanusDuplicatePolicyMode::ClassifierOnly => FilterDescriptor {
            name: "scott_native_hashkey",
            kind: FilterKind::BinaryIdentity,
            deterministic: true,
            output_mode: "binary",
            active: false,
            notes: "Exact dreadnaut/native hashkey filtering disabled for this lane; staged GA falls back to Rust-side PMOI and energy-tolerance duplicate control without atoms.in sidecars.",
        },
    };

    vec![
        identity,
        FilterDescriptor {
            name: "scott_intent_pmoi",
            kind: FilterKind::ThresholdDuplicate,
            deterministic: true,
            output_mode: "binary_threshold",
            active: true,
            notes: "Mass-weighted normalized principal-moment fallback after exact native hashkey comparison; implemented by the Rust duplicate classifier rather than the external hashkey adapter.",
        },
        FilterDescriptor {
            name: "energy_tolerance",
            kind: FilterKind::ThresholdDuplicate,
            deterministic: true,
            output_mode: "binary_threshold",
            active: true,
            notes: "Final duplicate fallback on absolute energy difference after the exact-hashkey and PMOI checks.",
        },
    ]
}
