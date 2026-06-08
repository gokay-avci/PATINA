use anyhow::{anyhow, Result};
use patina_search::{accept_energy_transition, MonteCarloAcceptance};
use patina_surface::{
    SurfaceFace, SurfaceGenerationConfig, SurfaceGenerationResult, SurfaceParentStructure,
};
use patina_types::{Candidate, EvalResult, EvaluationRecord};
use serde::Serialize;
use std::collections::BTreeSet;
use std::path::PathBuf;

use super::ports::{
    SamplingEvaluationIntent, SamplingEvaluationPort, SamplingEvaluationRequest,
    SamplingWorkflowIntent, SurfaceGenerationPort, SurfaceGenerationRequest,
    SurfaceReconstructionArtifactSink,
};

#[derive(Debug, Clone)]
pub struct SurfaceReconstructionWorkflowRequest {
    pub source_candidate: Candidate,
    pub generation_config: SurfaceGenerationConfig,
    pub target_face: SurfaceFace,
    pub movable_species: Vec<String>,
    pub site_filter: SurfaceReconstructionSiteFilter,
    pub region_policy: SurfaceReconstructionRegionPolicy,
    pub movable_layer_count: Option<usize>,
    pub layer_z_tolerance_angstrom: f64,
    pub move_family: SurfaceReconstructionMoveFamily,
    pub steps: usize,
    pub temperature: f64,
    pub lateral_fractional_step: f64,
    pub outward_normal_step_angstrom: f64,
    pub inward_normal_step_angstrom: f64,
    pub evaluate_initial_state: bool,
    pub seed: u64,
    pub workdir: PathBuf,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum SurfaceReconstructionSiteFilter {
    DanglingOnly,
    DanglingPreferred,
    SurfaceFaceOnly,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum SurfaceReconstructionMoveFamily {
    LateralOnly,
    OutwardNormalOnly,
    LateralAndOutwardNormal,
    LateralAndBidirectionalNormal,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum SurfaceReconstructionRegionPolicy {
    FullFace,
    RelaxedRegion,
    TopLayerOnly,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum SurfaceReconstructionSiteClass {
    DanglingCandidate,
    SurfaceFace,
}

#[derive(Debug, Clone, Serialize)]
pub struct SurfaceReconstructionMovableSite {
    pub atom_index: usize,
    pub species: String,
    pub face: SurfaceFace,
    pub site_class: SurfaceReconstructionSiteClass,
    pub degree: Option<usize>,
    pub expected_min: Option<usize>,
}

impl SurfaceReconstructionWorkflowRequest {
    pub fn validate(&self) -> Result<()> {
        self.source_candidate.validate().map_err(|error| {
            anyhow!("invalid surface reconstruction source candidate: {error:?}")
        })?;
        self.generation_config.validate()?;
        if self.workdir.as_os_str().is_empty() {
            return Err(anyhow!("surface reconstruction workdir must not be empty"));
        }
        if !self.temperature.is_finite() || self.temperature < 0.0 {
            return Err(anyhow!(
                "surface reconstruction temperature must be finite and non-negative"
            ));
        }
        if !self.lateral_fractional_step.is_finite() || self.lateral_fractional_step < 0.0 {
            return Err(anyhow!(
                "surface reconstruction lateral_fractional_step must be finite and non-negative"
            ));
        }
        if !self.outward_normal_step_angstrom.is_finite() || self.outward_normal_step_angstrom < 0.0
        {
            return Err(anyhow!(
                "surface reconstruction outward_normal_step_angstrom must be finite and non-negative"
            ));
        }
        if !self.inward_normal_step_angstrom.is_finite() || self.inward_normal_step_angstrom < 0.0 {
            return Err(anyhow!(
                "surface reconstruction inward_normal_step_angstrom must be finite and non-negative"
            ));
        }
        if self.target_face == SurfaceFace::Both {
            return Err(anyhow!(
                "surface reconstruction workflow currently requires a single target face"
            ));
        }
        if !self.layer_z_tolerance_angstrom.is_finite() || self.layer_z_tolerance_angstrom <= 0.0 {
            return Err(anyhow!(
                "surface reconstruction layer_z_tolerance_angstrom must be finite and positive"
            ));
        }
        if matches!(
            self.region_policy,
            SurfaceReconstructionRegionPolicy::RelaxedRegion
        ) && self.movable_layer_count.unwrap_or(0) == 0
        {
            return Err(anyhow!(
                "surface reconstruction relaxed-region policy requires movable_layer_count > 0"
            ));
        }
        if matches!(
            self.move_family,
            SurfaceReconstructionMoveFamily::LateralOnly
                | SurfaceReconstructionMoveFamily::LateralAndOutwardNormal
                | SurfaceReconstructionMoveFamily::LateralAndBidirectionalNormal
        ) && self.lateral_fractional_step == 0.0
        {
            return Err(anyhow!(
                "surface reconstruction lateral move family requires lateral_fractional_step > 0"
            ));
        }
        if matches!(
            self.move_family,
            SurfaceReconstructionMoveFamily::OutwardNormalOnly
                | SurfaceReconstructionMoveFamily::LateralAndOutwardNormal
        ) && self.outward_normal_step_angstrom == 0.0
        {
            return Err(anyhow!(
                "surface reconstruction outward-normal move family requires outward_normal_step_angstrom > 0"
            ));
        }
        if matches!(
            self.move_family,
            SurfaceReconstructionMoveFamily::LateralAndBidirectionalNormal
        ) && self.outward_normal_step_angstrom == 0.0
            && self.inward_normal_step_angstrom == 0.0
        {
            return Err(anyhow!(
                "surface reconstruction bidirectional-normal move family requires at least one non-zero normal step amplitude"
            ));
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct SurfaceReconstructionSummary {
    pub target_face: SurfaceFace,
    pub movable_species: Vec<String>,
    pub movable_indices: Vec<usize>,
    pub movable_sites: Vec<SurfaceReconstructionMovableSite>,
    pub site_filter: SurfaceReconstructionSiteFilter,
    pub region_policy: SurfaceReconstructionRegionPolicy,
    pub movable_layer_count: Option<usize>,
    pub layer_z_tolerance_angstrom: f64,
    pub move_family: SurfaceReconstructionMoveFamily,
    pub steps: usize,
    pub accepted_steps: usize,
    pub rejected_acceptance_steps: usize,
    pub rejected_evaluation_steps: usize,
    pub best_energy: Option<f64>,
    pub warnings: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct SurfaceReconstructionEvaluatedStructure {
    pub step_index: Option<usize>,
    pub moved_atom_index: Option<usize>,
    pub species: Option<String>,
    pub accepted: bool,
    pub evaluation: EvaluationRecord,
}

#[derive(Debug, Clone, Serialize)]
pub struct SurfaceReconstructionStepTrace {
    pub step_index: usize,
    pub moved_atom_index: usize,
    pub species: String,
    pub candidate_label: String,
    pub accepted: bool,
    pub energy: Option<f64>,
    pub reason: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct SurfaceReconstructionExecution {
    pub source_candidate: Candidate,
    pub parent: SurfaceParentStructure,
    pub generation_config: SurfaceGenerationConfig,
    pub generation_result: SurfaceGenerationResult,
    pub summary: SurfaceReconstructionSummary,
    pub initial_evaluation: Option<EvaluationRecord>,
    pub best_evaluation: Option<EvaluationRecord>,
    pub evaluated_structures: Vec<SurfaceReconstructionEvaluatedStructure>,
    pub final_candidate: Candidate,
    pub trace: Vec<SurfaceReconstructionStepTrace>,
}

pub fn run_surface_reconstruction_workflow(
    request: &SurfaceReconstructionWorkflowRequest,
    surface_port: &impl SurfaceGenerationPort,
    evaluation_port: &impl SamplingEvaluationPort,
    sink: &impl SurfaceReconstructionArtifactSink,
) -> Result<SurfaceReconstructionExecution> {
    let execution =
        execute_surface_reconstruction_workflow(request, surface_port, evaluation_port)?;
    sink.persist_surface_reconstruction_run(&execution)?;
    Ok(execution)
}

fn execute_surface_reconstruction_workflow(
    request: &SurfaceReconstructionWorkflowRequest,
    surface_port: &impl SurfaceGenerationPort,
    evaluation_port: &impl SamplingEvaluationPort,
) -> Result<SurfaceReconstructionExecution> {
    request.validate()?;

    let parent = SurfaceParentStructure::try_from_candidate(&request.source_candidate)?;
    let generation_result = surface_port.generate_surface(&SurfaceGenerationRequest {
        parent: parent.clone(),
        config: request.generation_config.clone(),
    })?;

    let movable_sites = select_movable_sites(
        &generation_result,
        request.target_face,
        &request.movable_species,
        request.site_filter,
        request.region_policy,
        request.movable_layer_count,
        request.layer_z_tolerance_angstrom,
    )?;
    let movable_indices = movable_sites
        .iter()
        .map(|site| site.atom_index)
        .collect::<Vec<_>>();
    if movable_indices.is_empty() {
        return Err(anyhow!(
            "surface reconstruction found no movable ions on {:?} face",
            request.target_face
        ));
    }

    let mut rng = ReconstructionRng::new(request.seed);
    let mut current_candidate = generation_result.slab.to_candidate();
    let mut current_eval: Option<EvalResult> = None;
    let mut initial_evaluation = None;
    let mut best_evaluation = None;
    let mut best_energy = None;
    let mut accepted_steps = 0usize;
    let mut rejected_acceptance_steps = 0usize;
    let mut rejected_evaluation_steps = 0usize;
    let mut evaluated_structures = Vec::with_capacity(request.steps + 1);
    let mut trace = Vec::with_capacity(request.steps);
    let mut warnings = Vec::new();

    if request.evaluate_initial_state {
        match evaluation_port.evaluate(&SamplingEvaluationRequest {
            candidate: current_candidate.clone(),
            eval_dir: request.workdir.join("initial"),
            workflow: SamplingWorkflowIntent::SurfaceReconstruction,
            intent: SamplingEvaluationIntent::InitialState,
            step_index: None,
            lid_index: None,
            runner_index: None,
        }) {
            Ok(result) => {
                best_energy = Some(result.energy);
                let evaluation_record = EvaluationRecord::from(&result);
                current_candidate = result.relaxed_candidate.clone();
                current_eval = Some(result.clone());
                initial_evaluation = Some(evaluation_record.clone());
                best_evaluation = Some(evaluation_record.clone());
                evaluated_structures.push(SurfaceReconstructionEvaluatedStructure {
                    step_index: None,
                    moved_atom_index: None,
                    species: None,
                    accepted: true,
                    evaluation: evaluation_record,
                });
            }
            Err(error) => {
                rejected_evaluation_steps += 1;
                warnings.push(format!(
                    "initial surface reconstruction evaluation failed; continuing from generated slab without a baseline energy: {error}"
                ));
            }
        }
    }

    for step_index in 0..request.steps {
        let chosen = movable_indices[(rng.next_u64() as usize) % movable_indices.len()];
        let proposal = propose_surface_reconstruction_candidate(
            &current_candidate,
            &generation_result,
            chosen,
            request,
            &mut rng,
            step_index,
        )?;

        let eval_request = SamplingEvaluationRequest {
            candidate: proposal.clone(),
            eval_dir: request.workdir.join(format!("step_{step_index:04}")),
            workflow: SamplingWorkflowIntent::SurfaceReconstruction,
            intent: SamplingEvaluationIntent::SamplingStep,
            step_index: Some(step_index),
            lid_index: None,
            runner_index: None,
        };
        eval_request.validate_shape()?;

        let result = match evaluation_port.evaluate(&eval_request) {
            Ok(result) => result,
            Err(error) => {
                rejected_evaluation_steps += 1;
                trace.push(SurfaceReconstructionStepTrace {
                    step_index,
                    moved_atom_index: chosen,
                    species: current_candidate.species[chosen].clone(),
                    candidate_label: proposal.label.clone(),
                    accepted: false,
                    energy: None,
                    reason: Some(error.to_string()),
                });
                continue;
            }
        };

        let evaluation_record = EvaluationRecord::from(&result);
        let is_new_best = best_energy
            .map(|best| result.energy <= best)
            .unwrap_or(true);
        best_energy = Some(
            best_energy
                .map(|best| best.min(result.energy))
                .unwrap_or(result.energy),
        );
        if is_new_best {
            best_evaluation = Some(evaluation_record.clone());
        }
        let accepted = accept_energy_transition(
            current_eval.as_ref().map(|value| value.energy),
            result.energy,
            MonteCarloAcceptance::Metropolis {
                temperature: request.temperature.max(1.0e-12),
            },
            rng.next_unit(),
        )
        .accepted;

        if accepted {
            accepted_steps += 1;
            current_candidate = result.relaxed_candidate.clone();
            current_eval = Some(result.clone());
        } else {
            rejected_acceptance_steps += 1;
        }
        evaluated_structures.push(SurfaceReconstructionEvaluatedStructure {
            step_index: Some(step_index),
            moved_atom_index: Some(chosen),
            species: Some(current_candidate.species[chosen].clone()),
            accepted,
            evaluation: evaluation_record,
        });

        trace.push(SurfaceReconstructionStepTrace {
            step_index,
            moved_atom_index: chosen,
            species: current_candidate.species[chosen].clone(),
            candidate_label: result.relaxed_candidate.label.clone(),
            accepted,
            energy: Some(result.energy),
            reason: if accepted {
                None
            } else {
                Some("rejected by surface reconstruction acceptance policy".into())
            },
        });
    }

    Ok(SurfaceReconstructionExecution {
        source_candidate: request.source_candidate.clone(),
        parent,
        generation_config: request.generation_config.clone(),
        generation_result,
        summary: SurfaceReconstructionSummary {
            target_face: request.target_face,
            movable_species: request.movable_species.clone(),
            movable_indices,
            movable_sites,
            site_filter: request.site_filter,
            region_policy: request.region_policy,
            movable_layer_count: request.movable_layer_count,
            layer_z_tolerance_angstrom: request.layer_z_tolerance_angstrom,
            move_family: request.move_family,
            steps: request.steps,
            accepted_steps,
            rejected_acceptance_steps,
            rejected_evaluation_steps,
            best_energy,
            warnings,
        },
        initial_evaluation,
        best_evaluation,
        evaluated_structures,
        final_candidate: current_candidate,
        trace,
    })
}

fn select_movable_sites(
    generation_result: &SurfaceGenerationResult,
    target_face: SurfaceFace,
    movable_species: &[String],
    site_filter: SurfaceReconstructionSiteFilter,
    region_policy: SurfaceReconstructionRegionPolicy,
    movable_layer_count: Option<usize>,
    layer_z_tolerance_angstrom: f64,
) -> Result<Vec<SurfaceReconstructionMovableSite>> {
    let summary = generation_result
        .diagnostics
        .surface_bond_summary
        .as_ref()
        .ok_or_else(|| anyhow!("surface generation result is missing surface_bond_summary"))?;

    let species_filter = movable_species
        .iter()
        .map(|value| value.as_str())
        .collect::<BTreeSet<_>>();
    let mut sites = summary
        .dangling_candidates
        .iter()
        .filter(|candidate| candidate.region == target_face)
        .filter(|candidate| {
            species_filter.is_empty() || species_filter.contains(candidate.species.as_str())
        })
        .map(|candidate| SurfaceReconstructionMovableSite {
            atom_index: candidate.atom_index,
            species: candidate.species.clone(),
            face: candidate.region,
            site_class: SurfaceReconstructionSiteClass::DanglingCandidate,
            degree: Some(candidate.degree),
            expected_min: Some(candidate.expected_min),
        })
        .collect::<Vec<_>>();

    let include_face_sites = matches!(
        site_filter,
        SurfaceReconstructionSiteFilter::DanglingPreferred
            | SurfaceReconstructionSiteFilter::SurfaceFaceOnly
    );
    let should_fallback_to_face = matches!(
        site_filter,
        SurfaceReconstructionSiteFilter::DanglingPreferred
    ) && sites.is_empty();

    if include_face_sites
        && (matches!(
            site_filter,
            SurfaceReconstructionSiteFilter::SurfaceFaceOnly
        ) || should_fallback_to_face)
    {
        let face_indices = match target_face {
            SurfaceFace::Top => &summary.top_indices,
            SurfaceFace::Bottom => &summary.bottom_indices,
            SurfaceFace::Both => unreachable!(),
        };
        sites = face_indices
            .iter()
            .copied()
            .filter(|index| {
                species_filter.is_empty()
                    || species_filter
                        .contains(generation_result.slab.atoms[*index].species.as_str())
            })
            .map(|atom_index| {
                let atom = &generation_result.slab.atoms[atom_index];
                SurfaceReconstructionMovableSite {
                    atom_index,
                    species: atom.species.clone(),
                    face: target_face,
                    site_class: SurfaceReconstructionSiteClass::SurfaceFace,
                    degree: None,
                    expected_min: None,
                }
            })
            .collect();
    }

    sites.sort_by_key(|site| site.atom_index);
    sites.dedup_by_key(|site| site.atom_index);
    filter_sites_by_region(
        &sites,
        generation_result,
        target_face,
        region_policy,
        movable_layer_count,
        layer_z_tolerance_angstrom,
    )
}

fn filter_sites_by_region(
    sites: &[SurfaceReconstructionMovableSite],
    generation_result: &SurfaceGenerationResult,
    target_face: SurfaceFace,
    region_policy: SurfaceReconstructionRegionPolicy,
    movable_layer_count: Option<usize>,
    layer_z_tolerance_angstrom: f64,
) -> Result<Vec<SurfaceReconstructionMovableSite>> {
    if matches!(region_policy, SurfaceReconstructionRegionPolicy::FullFace) {
        return Ok(sites.to_vec());
    }

    let selected_layers = match region_policy {
        SurfaceReconstructionRegionPolicy::FullFace => unreachable!(),
        SurfaceReconstructionRegionPolicy::TopLayerOnly => 1,
        SurfaceReconstructionRegionPolicy::RelaxedRegion => movable_layer_count.unwrap_or(1),
    };
    let allowed_indices = allowed_region_atom_indices(
        &generation_result.slab,
        target_face,
        selected_layers,
        layer_z_tolerance_angstrom,
    )?;
    Ok(sites
        .iter()
        .filter(|site| allowed_indices.contains(&site.atom_index))
        .cloned()
        .collect())
}

fn allowed_region_atom_indices(
    slab: &patina_surface::SurfaceSlab,
    target_face: SurfaceFace,
    selected_layers: usize,
    layer_z_tolerance_angstrom: f64,
) -> Result<BTreeSet<usize>> {
    if slab.atoms.is_empty() {
        return Err(anyhow!("surface reconstruction slab contains no atoms"));
    }

    let mut atoms_by_surface_z = slab
        .atoms
        .iter()
        .enumerate()
        .map(|(index, atom)| (index, atom.cartesian[2]))
        .collect::<Vec<_>>();
    match target_face {
        SurfaceFace::Top => atoms_by_surface_z.sort_by(|left, right| right.1.total_cmp(&left.1)),
        SurfaceFace::Bottom => atoms_by_surface_z.sort_by(|left, right| left.1.total_cmp(&right.1)),
        SurfaceFace::Both => unreachable!(),
    }

    let mut layers = Vec::<Vec<usize>>::new();
    let mut current_layer = Vec::<usize>::new();
    let mut current_reference_z: Option<f64> = None;
    for (index, z) in atoms_by_surface_z {
        match current_reference_z {
            None => {
                current_reference_z = Some(z);
                current_layer.push(index);
            }
            Some(reference_z) if (z - reference_z).abs() <= layer_z_tolerance_angstrom => {
                current_layer.push(index);
            }
            Some(_) => {
                layers.push(std::mem::take(&mut current_layer));
                current_reference_z = Some(z);
                current_layer.push(index);
            }
        }
    }
    if !current_layer.is_empty() {
        layers.push(current_layer);
    }

    Ok(layers.into_iter().take(selected_layers).flatten().collect())
}

fn propose_surface_reconstruction_candidate(
    current_candidate: &Candidate,
    generation_result: &SurfaceGenerationResult,
    moved_atom_index: usize,
    request: &SurfaceReconstructionWorkflowRequest,
    rng: &mut ReconstructionRng,
    step_index: usize,
) -> Result<Candidate> {
    let mut candidate = current_candidate.clone();
    let slab = &generation_result.slab;
    let c_norm = {
        let c = slab.lattice[0][2]
            .hypot(slab.lattice[1][2])
            .hypot(slab.lattice[2][2]);
        c.max(1.0e-9)
    };

    let (dx, dy) = if matches!(
        request.move_family,
        SurfaceReconstructionMoveFamily::LateralOnly
            | SurfaceReconstructionMoveFamily::LateralAndOutwardNormal
            | SurfaceReconstructionMoveFamily::LateralAndBidirectionalNormal
    ) {
        (
            (rng.next_unit() * 2.0 - 1.0) * request.lateral_fractional_step,
            (rng.next_unit() * 2.0 - 1.0) * request.lateral_fractional_step,
        )
    } else {
        (0.0, 0.0)
    };

    let outward_sign = match request.target_face {
        SurfaceFace::Top => 1.0,
        SurfaceFace::Bottom => -1.0,
        SurfaceFace::Both => 0.0,
    };
    let inward_sign = -outward_sign;
    let dz_angstrom = match request.move_family {
        SurfaceReconstructionMoveFamily::LateralOnly => 0.0,
        SurfaceReconstructionMoveFamily::OutwardNormalOnly
        | SurfaceReconstructionMoveFamily::LateralAndOutwardNormal => {
            outward_sign * request.outward_normal_step_angstrom * rng.next_unit()
        }
        SurfaceReconstructionMoveFamily::LateralAndBidirectionalNormal => {
            if rng.next_unit() < 0.5 {
                outward_sign * request.outward_normal_step_angstrom * rng.next_unit()
            } else {
                inward_sign * request.inward_normal_step_angstrom * rng.next_unit()
            }
        }
    };
    let dz = dz_angstrom / c_norm;

    let coords = &mut candidate.fractional_coords[moved_atom_index];
    coords[0] = (coords[0] + dx).rem_euclid(1.0);
    coords[1] = (coords[1] + dy).rem_euclid(1.0);
    coords[2] = (coords[2] + dz).clamp(0.0, 1.0);
    candidate.label = format!(
        "{}__surface_recon_step_{step_index:04}_atom_{moved_atom_index:04}",
        current_candidate.label
    );
    candidate
        .validate()
        .map_err(|error| anyhow!("invalid surface reconstruction proposal: {error:?}"))?;
    Ok(candidate)
}

#[derive(Debug, Clone)]
struct ReconstructionRng {
    state: u64,
}

impl ReconstructionRng {
    fn new(seed: u64) -> Self {
        Self {
            state: seed ^ 0x9E37_79B9_7F4A_7C15,
        }
    }

    fn next_u64(&mut self) -> u64 {
        self.state = self
            .state
            .wrapping_mul(6364136223846793005)
            .wrapping_add(1442695040888963407);
        self.state
    }

    fn next_unit(&mut self) -> f64 {
        let value = self.next_u64() >> 11;
        (value as f64) / ((1u64 << 53) as f64)
    }
}

#[cfg(test)]
mod tests {
    use super::{
        allowed_region_atom_indices, propose_surface_reconstruction_candidate,
        run_surface_reconstruction_workflow, ReconstructionRng, SurfaceReconstructionArtifactSink,
        SurfaceReconstructionExecution, SurfaceReconstructionMoveFamily,
        SurfaceReconstructionRegionPolicy, SurfaceReconstructionSiteClass,
        SurfaceReconstructionSiteFilter, SurfaceReconstructionWorkflowRequest,
    };
    use crate::application::ports::{
        SamplingEvaluationPort, SurfaceGenerationPort, SurfaceGenerationRequest,
    };
    use anyhow::Result;
    use patina_surface::{
        MillerIndex, SlabReductionConfig, SurfaceAtom, SurfaceBondDiagnosticsSummary,
        SurfaceDanglingBondCandidate, SurfaceDiagnosticsDataset, SurfaceFace,
        SurfaceGenerationConfig, SurfaceGenerationResult, SurfaceReconstructionMode, SurfaceSlab,
        SurfaceSupercellConfig, SurfaceTerminationBias, SurfaceTopologyDiagnostics,
    };
    use patina_types::{Candidate, EvalResult};
    use std::cell::RefCell;
    use std::path::PathBuf;

    struct StubSurfacePort;

    impl SurfaceGenerationPort for StubSurfacePort {
        fn generate_surface(
            &self,
            request: &SurfaceGenerationRequest,
        ) -> Result<SurfaceGenerationResult> {
            Ok(SurfaceGenerationResult {
                slab: SurfaceSlab {
                    label: format!("{}__surface", request.parent.label),
                    parent_label: request.parent.label.clone(),
                    miller: request.config.miller.clone(),
                    lattice: [[5.0, 0.0, 0.0], [0.0, 5.0, 0.0], [0.0, 0.0, 20.0]],
                    periodic_axes: [true, true, false],
                    atoms: vec![
                        SurfaceAtom {
                            species: "Zn".into(),
                            fractional: [0.2, 0.2, 0.2],
                            cartesian: [1.0, 1.0, 4.0],
                            source_fractional: None,
                        },
                        SurfaceAtom {
                            species: "O".into(),
                            fractional: [0.3, 0.3, 0.8],
                            cartesian: [1.5, 1.5, 16.0],
                            source_fractional: None,
                        },
                    ],
                    thickness_angstrom: request.config.thickness_angstrom,
                    vacuum_angstrom: request.config.vacuum_angstrom,
                },
                diagnostics: SurfaceTopologyDiagnostics {
                    topology_safe_cut: Some(true),
                    broken_bond_estimate: Some(1),
                    dedup_report: None,
                    chosen_cut_offset_angstrom: Some(0.0),
                    interplanar_spacing_angstrom: Some(2.5),
                    layer_count: Some(4),
                    graph_diagnostics: Some(SurfaceDiagnosticsDataset {
                        n_atoms: 2,
                        n_bonds: 0,
                        n_components: 2,
                        largest_component: 1,
                        element_counts: vec![("O".into(), 1), ("Zn".into(), 1)],
                        element_degrees: vec![],
                        isolated_atoms: vec![0, 1],
                        suspicious_atoms: vec![],
                    }),
                    surface_bond_summary: Some(SurfaceBondDiagnosticsSummary {
                        n_atoms: 2,
                        z_skin_angstrom: 3.0,
                        z_min: 4.0,
                        z_max: 16.0,
                        bottom_indices: vec![0],
                        top_indices: vec![1],
                        dangling_candidates: vec![SurfaceDanglingBondCandidate {
                            atom_index: 1,
                            species: "O".into(),
                            degree: 0,
                            expected_min: 1,
                            region: SurfaceFace::Top,
                        }],
                        surface_stats: vec![("O".into(), (1, 0.0))],
                    }),
                    warnings: Vec::new(),
                },
            })
        }
    }

    #[derive(Default)]
    struct RecordingEvalPort {
        labels: RefCell<Vec<String>>,
    }

    impl SamplingEvaluationPort for RecordingEvalPort {
        fn evaluate(
            &self,
            request: &crate::application::ports::SamplingEvaluationRequest,
        ) -> Result<EvalResult> {
            self.labels
                .borrow_mut()
                .push(request.candidate.label.clone());
            Ok(EvalResult {
                energy: -(self.labels.borrow().len() as f64),
                forces: vec![[0.0, 0.0, 0.0]; request.candidate.species.len()],
                relaxed_candidate: request.candidate.clone(),
                converged: true,
                wall_time: std::time::Duration::from_secs(0),
            })
        }
    }

    #[derive(Default)]
    struct NoopSink;

    impl SurfaceReconstructionArtifactSink for NoopSink {
        fn persist_surface_reconstruction_run(
            &self,
            _execution: &SurfaceReconstructionExecution,
        ) -> Result<()> {
            Ok(())
        }
    }

    fn source_candidate() -> Candidate {
        Candidate::fully_periodic(
            "framework",
            vec!["Zn".into(), "O".into()],
            vec![[0.0, 0.0, 0.0], [0.5, 0.5, 0.5]],
            [[4.2, 0.0, 0.0], [0.0, 4.2, 0.0], [0.0, 0.0, 4.2]],
        )
    }

    #[test]
    fn workflow_uses_surface_bond_summary_to_pick_movable_ions() {
        let eval = RecordingEvalPort::default();
        let execution = run_surface_reconstruction_workflow(
            &SurfaceReconstructionWorkflowRequest {
                source_candidate: source_candidate(),
                generation_config: SurfaceGenerationConfig {
                    miller: MillerIndex::new(0, 0, 1).expect("miller"),
                    thickness_angstrom: 8.0,
                    vacuum_angstrom: 10.0,
                    supercell: SurfaceSupercellConfig::default(),
                    cut_strategy: patina_surface::SurfaceCutStrategy::TopologyAware,
                    cut_offset_fraction: None,
                    slab_reduction: SlabReductionConfig::default(),
                    reconstruction: SurfaceReconstructionMode::None,
                    termination_bias: SurfaceTerminationBias::Neutral,
                },
                target_face: SurfaceFace::Top,
                movable_species: vec!["O".into()],
                site_filter: SurfaceReconstructionSiteFilter::DanglingOnly,
                region_policy: SurfaceReconstructionRegionPolicy::FullFace,
                movable_layer_count: None,
                layer_z_tolerance_angstrom: 0.5,
                move_family: SurfaceReconstructionMoveFamily::LateralAndOutwardNormal,
                steps: 2,
                temperature: 100.0,
                lateral_fractional_step: 0.05,
                outward_normal_step_angstrom: 1.0,
                inward_normal_step_angstrom: 0.25,
                evaluate_initial_state: true,
                seed: 7,
                workdir: PathBuf::from("/tmp/surface_reconstruction"),
            },
            &StubSurfacePort,
            &eval,
            &NoopSink,
        )
        .expect("workflow");

        assert_eq!(execution.summary.movable_indices, vec![1]);
        assert_eq!(execution.summary.movable_sites.len(), 1);
        assert_eq!(
            execution.summary.movable_sites[0].site_class,
            SurfaceReconstructionSiteClass::DanglingCandidate
        );
        assert_eq!(eval.labels.borrow().len(), 3);
        assert_eq!(execution.summary.accepted_steps, 2);
        assert!(execution.summary.best_energy.is_some());
    }

    #[test]
    fn workflow_can_select_surface_face_sites_explicitly() {
        let eval = RecordingEvalPort::default();
        let execution = run_surface_reconstruction_workflow(
            &SurfaceReconstructionWorkflowRequest {
                source_candidate: source_candidate(),
                generation_config: SurfaceGenerationConfig {
                    miller: MillerIndex::new(0, 0, 1).expect("miller"),
                    thickness_angstrom: 8.0,
                    vacuum_angstrom: 10.0,
                    supercell: SurfaceSupercellConfig::default(),
                    cut_strategy: patina_surface::SurfaceCutStrategy::TopologyAware,
                    cut_offset_fraction: None,
                    slab_reduction: SlabReductionConfig::default(),
                    reconstruction: SurfaceReconstructionMode::None,
                    termination_bias: SurfaceTerminationBias::Neutral,
                },
                target_face: SurfaceFace::Top,
                movable_species: vec!["O".into()],
                site_filter: SurfaceReconstructionSiteFilter::SurfaceFaceOnly,
                region_policy: SurfaceReconstructionRegionPolicy::FullFace,
                movable_layer_count: None,
                layer_z_tolerance_angstrom: 0.5,
                move_family: SurfaceReconstructionMoveFamily::LateralOnly,
                steps: 1,
                temperature: 100.0,
                lateral_fractional_step: 0.05,
                outward_normal_step_angstrom: 1.0,
                inward_normal_step_angstrom: 0.25,
                evaluate_initial_state: false,
                seed: 7,
                workdir: PathBuf::from("/tmp/surface_reconstruction"),
            },
            &StubSurfacePort,
            &eval,
            &NoopSink,
        )
        .expect("workflow");

        assert_eq!(execution.summary.movable_indices, vec![1]);
        assert_eq!(
            execution.summary.movable_sites[0].site_class,
            SurfaceReconstructionSiteClass::SurfaceFace
        );
    }

    #[test]
    fn lateral_only_move_family_preserves_fractional_z() {
        let current_candidate = Candidate::fully_periodic(
            "slab",
            vec!["O".into()],
            vec![[0.3, 0.3, 0.8]],
            [[5.0, 0.0, 0.0], [0.0, 5.0, 0.0], [0.0, 0.0, 20.0]],
        );
        let generation_result = StubSurfacePort
            .generate_surface(&SurfaceGenerationRequest {
                parent: patina_surface::SurfaceParentStructure::try_from_candidate(
                    &source_candidate(),
                )
                .expect("parent"),
                config: SurfaceGenerationConfig {
                    miller: MillerIndex::new(0, 0, 1).expect("miller"),
                    thickness_angstrom: 8.0,
                    vacuum_angstrom: 10.0,
                    supercell: SurfaceSupercellConfig::default(),
                    cut_strategy: patina_surface::SurfaceCutStrategy::TopologyAware,
                    cut_offset_fraction: None,
                    slab_reduction: SlabReductionConfig::default(),
                    reconstruction: SurfaceReconstructionMode::None,
                    termination_bias: SurfaceTerminationBias::Neutral,
                },
            })
            .expect("surface");

        let mut rng = ReconstructionRng::new(11);
        let proposal = propose_surface_reconstruction_candidate(
            &current_candidate,
            &generation_result,
            0,
            &SurfaceReconstructionWorkflowRequest {
                source_candidate: source_candidate(),
                generation_config: SurfaceGenerationConfig {
                    miller: MillerIndex::new(0, 0, 1).expect("miller"),
                    thickness_angstrom: 8.0,
                    vacuum_angstrom: 10.0,
                    supercell: SurfaceSupercellConfig::default(),
                    cut_strategy: patina_surface::SurfaceCutStrategy::TopologyAware,
                    cut_offset_fraction: None,
                    slab_reduction: SlabReductionConfig::default(),
                    reconstruction: SurfaceReconstructionMode::None,
                    termination_bias: SurfaceTerminationBias::Neutral,
                },
                target_face: SurfaceFace::Top,
                movable_species: vec!["O".into()],
                site_filter: SurfaceReconstructionSiteFilter::DanglingOnly,
                region_policy: SurfaceReconstructionRegionPolicy::FullFace,
                movable_layer_count: None,
                layer_z_tolerance_angstrom: 0.5,
                move_family: SurfaceReconstructionMoveFamily::LateralOnly,
                steps: 1,
                temperature: 100.0,
                lateral_fractional_step: 0.05,
                outward_normal_step_angstrom: 1.0,
                inward_normal_step_angstrom: 0.25,
                evaluate_initial_state: false,
                seed: 11,
                workdir: PathBuf::from("/tmp/surface_reconstruction"),
            },
            &mut rng,
            0,
        )
        .expect("proposal");

        assert!(
            (proposal.fractional_coords[0][2] - current_candidate.fractional_coords[0][2]).abs()
                < 1.0e-12
        );
    }

    #[test]
    fn top_layer_region_policy_limits_selection_to_outermost_layer() {
        let slab = SurfaceSlab {
            label: "slab".into(),
            parent_label: "parent".into(),
            miller: MillerIndex::new(0, 0, 1).expect("miller"),
            lattice: [[5.0, 0.0, 0.0], [0.0, 5.0, 0.0], [0.0, 0.0, 20.0]],
            periodic_axes: [true, true, false],
            atoms: vec![
                SurfaceAtom {
                    species: "O".into(),
                    fractional: [0.1, 0.1, 0.9],
                    cartesian: [0.5, 0.5, 18.0],
                    source_fractional: None,
                },
                SurfaceAtom {
                    species: "O".into(),
                    fractional: [0.2, 0.2, 0.78],
                    cartesian: [1.0, 1.0, 15.6],
                    source_fractional: None,
                },
                SurfaceAtom {
                    species: "Mg".into(),
                    fractional: [0.3, 0.3, 0.18],
                    cartesian: [1.5, 1.5, 3.6],
                    source_fractional: None,
                },
            ],
            thickness_angstrom: 12.0,
            vacuum_angstrom: 10.0,
        };

        let top_only = allowed_region_atom_indices(&slab, SurfaceFace::Top, 1, 0.6).expect("top");
        let relaxed_two =
            allowed_region_atom_indices(&slab, SurfaceFace::Top, 2, 0.6).expect("two");

        assert_eq!(top_only, [0usize].into_iter().collect());
        assert_eq!(relaxed_two, [0usize, 1usize].into_iter().collect());
    }
}
