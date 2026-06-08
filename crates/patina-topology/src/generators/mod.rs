mod barrel;
mod common;
mod constrained_random;
mod platonic;
mod random_graph;
mod ring;
mod shell;
mod topology_mc;
mod wire;

pub use barrel::BarrelGenerator;
pub use constrained_random::{
    ChemicalEdgePolicy, ConstrainedRandomGraphGenerator, ConstrainedRandomReport, DegreeBounds,
};
pub use platonic::PlatonicGenerator;
pub use random_graph::RandomGraphGenerator;
pub use ring::RingGenerator;
pub use shell::ShellGenerator;
pub use topology_mc::{TopologyMonteCarloGenerator, TopologyMonteCarloReport};
pub use wire::WireGenerator;

use crate::domain::{
    Composition, ElementSymbol, GeometryValidationConfig, MotifCandidate, TopologyError,
    TopologyResult,
};
use std::collections::BTreeMap;

#[derive(Debug, Clone)]
pub struct GenerationRequest {
    pub formula: String,
    pub n_min: usize,
    pub n_max: usize,
    pub bond_length: f64,
    pub bond_length_source: Option<String>,
    pub generator_names: Vec<String>,
    pub max_candidates: usize,
    pub seed: Option<u64>,
    pub constraints: GenerationConstraints,
}

#[derive(Debug, Clone)]
pub struct GenerationConstraints {
    pub validation: GeometryValidationConfig,
    pub degree_bounds: BTreeMap<ElementSymbol, DegreeBounds>,
    pub fallback_degree_bounds: DegreeBounds,
    pub edge_policy: ChemicalEdgePolicy,
    pub target_extra_edges: Option<usize>,
    pub mc_proposals: usize,
    pub mc_max_accept: usize,
    pub mc_min_jaccard_distance: f64,
}

impl GenerationConstraints {
    pub fn for_bond_length(bond_length: f64) -> Self {
        Self {
            validation: GeometryValidationConfig::for_bond_length(bond_length),
            degree_bounds: BTreeMap::new(),
            fallback_degree_bounds: DegreeBounds { min: 1, max: 4 },
            edge_policy: ChemicalEdgePolicy::PreferHetero,
            target_extra_edges: None,
            mc_proposals: 32,
            mc_max_accept: 8,
            mc_min_jaccard_distance: 0.20,
        }
    }
}

pub fn generate_candidates(request: &GenerationRequest) -> TopologyResult<Vec<MotifCandidate>> {
    let mut candidates = Vec::new();
    let mut next_index = 1usize;
    for n in request.n_min..=request.n_max {
        let composition = Composition::from_formula(&request.formula, n)?;
        for name in &request.generator_names {
            if candidates.len() >= request.max_candidates {
                return Ok(candidates);
            }
            match name.trim().to_ascii_lowercase().as_str() {
                "ring" => {
                    if composition.total_atoms >= 3 {
                        candidates.push(RingGenerator::new(request.bond_length).generate(
                            next_index,
                            composition.clone(),
                            request.seed,
                        )?);
                        next_index += 1;
                    }
                }
                "barrel" => {
                    for candidate in BarrelGenerator::new(request.bond_length).generate_family(
                        next_index,
                        composition.clone(),
                        request.seed,
                    )? {
                        if candidates.len() >= request.max_candidates {
                            return Ok(candidates);
                        }
                        next_index += 1;
                        candidates.push(candidate);
                    }
                }
                "wire" => {
                    candidates.push(WireGenerator::new(request.bond_length).generate(
                        next_index,
                        composition.clone(),
                        request.seed,
                    )?);
                    next_index += 1;
                }
                "platonic" => {
                    for candidate in PlatonicGenerator::new(request.bond_length).generate_family(
                        next_index,
                        composition.clone(),
                        request.seed,
                    )? {
                        if candidates.len() >= request.max_candidates {
                            return Ok(candidates);
                        }
                        next_index += 1;
                        candidates.push(candidate);
                    }
                }
                "random" | "random_graph" => {
                    candidates.push(RandomGraphGenerator::new(request.bond_length).generate(
                        next_index,
                        composition.clone(),
                        request.seed,
                    )?);
                    next_index += 1;
                }
                "constrained_random" | "constrained_random_graph" => {
                    candidates.push(constrained_generator(request).generate(
                        next_index,
                        composition.clone(),
                        request.seed,
                    )?);
                    next_index += 1;
                }
                "topology_mc" | "mc" => {
                    for candidate in topology_mc_generator(request).generate_family(
                        next_index,
                        composition.clone(),
                        request.seed,
                    )? {
                        if candidates.len() >= request.max_candidates {
                            return Ok(candidates);
                        }
                        next_index += 1;
                        candidates.push(candidate);
                    }
                }
                "cage" | "closed_cage" => {
                    candidates.push(ShellGenerator::new(request.bond_length).closed_cage(
                        next_index,
                        composition.clone(),
                        request.seed,
                    )?);
                    next_index += 1;
                }
                "shell" | "hollow_shell" => {
                    candidates.push(ShellGenerator::new(request.bond_length).hollow_shell(
                        next_index,
                        composition.clone(),
                        request.seed,
                    )?);
                    next_index += 1;
                }
                "multi_shell" => {
                    candidates.push(ShellGenerator::new(request.bond_length).multi_shell(
                        next_index,
                        composition.clone(),
                        request.seed,
                    )?);
                    next_index += 1;
                }
                "onion" | "onion_like_shell" => {
                    candidates.push(ShellGenerator::new(request.bond_length).onion_like_shell(
                        next_index,
                        composition.clone(),
                        request.seed,
                    )?);
                    next_index += 1;
                }
                "face_capped_polyhedron" => {
                    candidates.push(
                        ShellGenerator::new(request.bond_length).face_capped_polyhedron(
                            next_index,
                            composition.clone(),
                            request.seed,
                        )?,
                    );
                    next_index += 1;
                }
                "edge_decorated_polyhedron" => {
                    candidates.push(
                        ShellGenerator::new(request.bond_length).edge_decorated_polyhedron(
                            next_index,
                            composition.clone(),
                            request.seed,
                        )?,
                    );
                    next_index += 1;
                }
                "" => {}
                other => {
                    return Err(TopologyError::UnsupportedGenerator {
                        name: other.to_string(),
                    });
                }
            }
        }
    }
    Ok(candidates)
}

fn constrained_generator(request: &GenerationRequest) -> ConstrainedRandomGraphGenerator {
    let mut generator = ConstrainedRandomGraphGenerator::new(request.bond_length);
    generator.degree_bounds = request.constraints.degree_bounds.clone();
    generator.fallback_bounds = request.constraints.fallback_degree_bounds.clone();
    generator.edge_policy = request.constraints.edge_policy.clone();
    generator.target_extra_edges = request.constraints.target_extra_edges.unwrap_or(0);
    generator.validation_config = request.constraints.validation.clone();
    generator
}

fn topology_mc_generator(request: &GenerationRequest) -> TopologyMonteCarloGenerator {
    let mut generator = TopologyMonteCarloGenerator::new(request.bond_length);
    generator.constrained = constrained_generator(request);
    generator.proposals = request.constraints.mc_proposals;
    generator.max_accept = request.constraints.mc_max_accept;
    generator.min_jaccard_distance = request.constraints.mc_min_jaccard_distance;
    generator
}
