use patina_sci_kernel::{
    cartesian_to_fractional, fractional_to_cartesian, minimum_image_cartesian_distance_sq,
    normalize_fractional_coordinate,
};
use patina_types::{Candidate, EvalResult};
use serde::{Deserialize, Serialize};

use crate::framework::PeriodicFramework;
use crate::properties::{
    cell_volume, deposit_equitable, increment_histogram, wrapped_grid_index, DensityGrid,
    DensityGridBinning, DensityGridNormalization, DensityGridSpec, EnergyHistogram,
    EnergyHistogramSpec, NumberHistogram, NumberHistogramSpec,
};
use crate::RaspaInterfaceError;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GcmcConditions {
    pub temperature_kelvin: f64,
    pub chemical_potential: Option<f64>,
    pub fugacity_pascal: Option<f64>,
    pub pressure_pascal: Option<f64>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GcmcMoveSchedule {
    pub translation_probability: f64,
    pub rotation_probability: f64,
    pub reinsertion_probability: f64,
    pub swap_probability: f64,
    pub widom_probability: f64,
}

impl Default for GcmcMoveSchedule {
    fn default() -> Self {
        Self {
            translation_probability: 1.0,
            rotation_probability: 1.0,
            reinsertion_probability: 1.0,
            swap_probability: 1.0,
            widom_probability: 0.0,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GuestAtom {
    pub species: String,
    pub cartesian: [f64; 3],
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RigidGuestTemplate {
    pub label: String,
    pub atoms: Vec<GuestAtom>,
}

impl RigidGuestTemplate {
    pub fn hydrogen() -> Self {
        Self {
            label: "h2".into(),
            atoms: vec![
                GuestAtom {
                    species: "H".into(),
                    cartesian: [0.0, 0.0, -0.3705],
                },
                GuestAtom {
                    species: "H".into(),
                    cartesian: [0.0, 0.0, 0.3705],
                },
            ],
        }
    }

    pub fn atom_count(&self) -> usize {
        self.atoms.len()
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GcmcRequest {
    pub framework: PeriodicFramework,
    pub guest_name: String,
    pub guest_template: RigidGuestTemplate,
    pub blocks: usize,
    pub initialization_cycles: usize,
    pub production_cycles: usize,
    pub conditions: GcmcConditions,
    pub move_schedule: GcmcMoveSchedule,
    pub random_seed: u64,
    pub max_guest_count: usize,
    pub minimum_guest_host_distance: f64,
    pub minimum_guest_guest_distance: f64,
    pub density_grid: Option<DensityGridSpec>,
    pub energy_histogram: Option<EnergyHistogramSpec>,
    pub number_histogram: Option<NumberHistogramSpec>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GcmcSummary {
    pub attempted_cycles: usize,
    pub accepted_insertions: usize,
    pub accepted_deletions: usize,
    pub accepted_translations: usize,
    pub accepted_rotations: usize,
    pub accepted_reinsertions: usize,
    pub accepted_widom: usize,
    pub rejected_moves: usize,
    pub mean_loading: f64,
    pub mean_energy: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GcmcTracePoint {
    pub cycle: usize,
    pub loading: usize,
    pub total_energy: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GcmcResult {
    pub summary: GcmcSummary,
    pub final_candidate: Candidate,
    pub density_grid: Option<DensityGrid>,
    pub energy_histogram: Option<EnergyHistogram>,
    pub number_histogram: Option<NumberHistogram>,
    pub production_trace: Vec<GcmcTracePoint>,
}

pub trait GcmcEngine {
    fn run(&self, request: &GcmcRequest) -> Result<GcmcResult, RaspaInterfaceError>;
}

pub trait GcmcEnergyEvaluator {
    fn evaluate_configuration(
        &self,
        candidate: &Candidate,
    ) -> Result<EvalResult, RaspaInterfaceError>;
}

#[derive(Debug, Clone)]
struct GuestPlacement {
    center_fractional: [f64; 3],
    rotation: [[f64; 3]; 3],
}

#[derive(Debug, Clone)]
struct Lcg64 {
    state: u64,
}

impl Lcg64 {
    fn new(seed: u64) -> Self {
        let state = if seed == 0 {
            0x9E37_79B9_7F4A_7C15
        } else {
            seed
        };
        Self { state }
    }

    fn next_u64(&mut self) -> u64 {
        self.state ^= self.state >> 12;
        self.state ^= self.state << 25;
        self.state ^= self.state >> 27;
        self.state = self.state.wrapping_mul(0x2545_F491_4F6C_DD1D);
        self.state
    }

    fn uniform(&mut self) -> f64 {
        const SCALE: f64 = 1.0 / ((1u64 << 53) as f64);
        ((self.next_u64() >> 11) as f64) * SCALE
    }

    fn range_usize(&mut self, upper: usize) -> usize {
        if upper <= 1 {
            0
        } else {
            (self.next_u64() as usize) % upper
        }
    }
}

#[derive(Debug, Clone)]
pub struct RigidGuestGcmcEngine<E> {
    evaluator: E,
}

impl<E> RigidGuestGcmcEngine<E> {
    pub fn new(evaluator: E) -> Self {
        Self { evaluator }
    }
}

impl<E: GcmcEnergyEvaluator> GcmcEngine for RigidGuestGcmcEngine<E> {
    fn run(&self, request: &GcmcRequest) -> Result<GcmcResult, RaspaInterfaceError> {
        validate_gcmc_request(request)?;

        let framework_candidate = request.framework.to_candidate();
        let lattice = request.framework.lattice;
        cartesian_to_fractional(lattice, [0.0, 0.0, 0.0]).ok_or_else(|| {
            RaspaInterfaceError::InvalidCandidate("framework lattice is singular".into())
        })?;
        let host_atom_count = request.framework.atom_count();
        let guest_atom_count = request.guest_template.atom_count();
        let beta = 1.0 / (8.617_333_262_145e-5 * request.conditions.temperature_kelvin);
        let activity = grand_canonical_activity(&request.conditions)?;

        let baseline = self
            .evaluator
            .evaluate_configuration(&framework_candidate)?;
        let mut placements = Vec::<GuestPlacement>::new();
        let mut current_candidate = framework_candidate.clone();
        let mut current_energy = baseline.energy;
        let mut rng = Lcg64::new(request.random_seed);

        let density_grid_channels = request
            .density_grid
            .as_ref()
            .map(|spec| {
                if spec.pseudo_atom_channels.is_empty() {
                    vec![request.guest_name.clone()]
                } else {
                    spec.pseudo_atom_channels.clone()
                }
            })
            .unwrap_or_default();
        let mut density_values = request.density_grid.as_ref().map(|spec| {
            let [nx, ny, nz] = spec.dimensions;
            vec![0.0; density_grid_channels.len().max(1) * nx * ny * nz]
        });
        let mut density_sample_count = 0usize;

        let mut energy_total = request
            .energy_histogram
            .as_ref()
            .map(|spec| vec![0.0; spec.number_of_bins]);
        let mut energy_vdw = request
            .energy_histogram
            .as_ref()
            .map(|spec| vec![0.0; spec.number_of_bins]);
        let mut energy_coulomb = request
            .energy_histogram
            .as_ref()
            .map(|spec| vec![0.0; spec.number_of_bins]);
        let energy_polarization = request
            .energy_histogram
            .as_ref()
            .map(|spec| vec![0.0; spec.number_of_bins]);

        let mut number_histogram = request
            .number_histogram
            .as_ref()
            .map(|spec| vec![vec![0.0; spec.upper_limit - spec.lower_limit + 1]]);
        let mut production_trace = Vec::with_capacity(request.production_cycles);

        let mut accepted_insertions = 0usize;
        let mut accepted_deletions = 0usize;
        let mut accepted_translations = 0usize;
        let mut accepted_rotations = 0usize;
        let mut accepted_reinsertions = 0usize;
        let mut accepted_widom = 0usize;
        let mut rejected_moves = 0usize;
        let mut production_energy_sum = 0.0;
        let mut production_loading_sum = 0.0;

        let total_cycles = request.initialization_cycles + request.production_cycles;
        for cycle in 0..total_cycles {
            let move_choice = choose_move(&request.move_schedule, placements.len(), &mut rng);
            let proposal =
                propose_move(&move_choice, &placements, request.max_guest_count, &mut rng);

            let accepted = if let Some((next_placements, delta_particles)) = proposal {
                let candidate = build_guest_configuration_candidate(
                    &request.framework,
                    &next_placements,
                    &request.guest_template,
                )?;
                if candidate_respects_minimum_distances(
                    &candidate,
                    host_atom_count,
                    guest_atom_count,
                    request.minimum_guest_host_distance,
                    request.minimum_guest_guest_distance,
                ) {
                    let evaluated = self.evaluator.evaluate_configuration(&candidate)?;
                    let delta_energy = evaluated.energy - current_energy;
                    let probability = acceptance_probability(
                        &move_choice,
                        beta,
                        delta_energy,
                        activity,
                        cell_volume(lattice).abs(),
                        placements.len(),
                        delta_particles,
                    );
                    if rng.uniform() < probability {
                        if matches!(move_choice, GcmcMove::Widom) {
                            accepted_widom += 1;
                        } else {
                            current_candidate = evaluated.relaxed_candidate;
                            current_energy = evaluated.energy;
                            placements = next_placements;
                            match move_choice.as_str() {
                                "swap_insert" => accepted_insertions += 1,
                                "swap_delete" => accepted_deletions += 1,
                                "translation" => accepted_translations += 1,
                                "rotation" => accepted_rotations += 1,
                                "reinsertion" => accepted_reinsertions += 1,
                                "widom" => {}
                                _ => {}
                            }
                        }
                        true
                    } else {
                        rejected_moves += 1;
                        false
                    }
                } else {
                    rejected_moves += 1;
                    false
                }
            } else {
                rejected_moves += 1;
                false
            };

            let _ = accepted;

            if cycle >= request.initialization_cycles {
                let production_cycle = cycle - request.initialization_cycles;
                let adsorption_energy = current_energy - baseline.energy;
                production_energy_sum += adsorption_energy;
                production_loading_sum += placements.len() as f64;
                production_trace.push(GcmcTracePoint {
                    cycle: production_cycle,
                    loading: placements.len(),
                    total_energy: adsorption_energy,
                });

                if let (Some(spec), Some(hist)) = (&request.energy_histogram, &mut energy_total) {
                    if production_cycle.is_multiple_of(spec.sample_every) {
                        increment_histogram(hist, adsorption_energy, spec.range);
                        if let Some(vdw) = &mut energy_vdw {
                            increment_histogram(vdw, adsorption_energy, spec.range);
                        }
                        if let Some(coulomb) = &mut energy_coulomb {
                            increment_histogram(coulomb, 0.0, spec.range);
                        }
                    }
                }

                if let (Some(spec), Some(hist)) = (&request.number_histogram, &mut number_histogram)
                {
                    if production_cycle.is_multiple_of(spec.sample_every) {
                        let loading = placements.len();
                        if loading >= spec.lower_limit && loading <= spec.upper_limit {
                            hist[0][loading - spec.lower_limit] += 1.0;
                        }
                    }
                }

                if let (Some(spec), Some(values)) = (&request.density_grid, &mut density_values) {
                    if production_cycle.is_multiple_of(spec.sample_every) && !placements.is_empty()
                    {
                        let channel = 0usize;
                        for placement in &placements {
                            match spec.binning {
                                DensityGridBinning::Standard => {
                                    let ix = wrapped_grid_index(
                                        placement.center_fractional[0],
                                        spec.dimensions[0],
                                    );
                                    let iy = wrapped_grid_index(
                                        placement.center_fractional[1],
                                        spec.dimensions[1],
                                    );
                                    let iz = wrapped_grid_index(
                                        placement.center_fractional[2],
                                        spec.dimensions[2],
                                    );
                                    let offset = ((channel * spec.dimensions[0] + ix)
                                        * spec.dimensions[1]
                                        + iy)
                                        * spec.dimensions[2]
                                        + iz;
                                    values[offset] += 1.0;
                                }
                                DensityGridBinning::Equitable => {
                                    deposit_equitable(
                                        values,
                                        &spec.dimensions,
                                        channel,
                                        placement.center_fractional,
                                        1,
                                    );
                                }
                            }
                        }
                        density_sample_count += placements.len();
                    }
                }
            }
        }

        let density_grid =
            if let (Some(spec), Some(mut values)) = (&request.density_grid, density_values) {
                match spec.normalization {
                    DensityGridNormalization::Max => {
                        let max_value = values.iter().copied().fold(0.0_f64, f64::max);
                        if max_value > 0.0 {
                            for value in &mut values {
                                *value /= max_value;
                            }
                        }
                    }
                    DensityGridNormalization::NumberDensity => {
                        let cell_vol = cell_volume(lattice).abs();
                        let normalization = if density_sample_count > 0 && cell_vol > 0.0 {
                            (spec.dimensions[0] * spec.dimensions[1] * spec.dimensions[2]) as f64
                                / (cell_vol * density_sample_count as f64)
                        } else {
                            1.0
                        };
                        for value in &mut values {
                            *value *= normalization;
                        }
                    }
                }
                Some(DensityGrid {
                    dimensions: spec.dimensions,
                    channels: density_grid_channels.len().max(1),
                    samples: density_sample_count,
                    values,
                })
            } else {
                None
            };

        let energy_histogram = if let Some(spec) = &request.energy_histogram {
            let mut total = energy_total.unwrap_or_else(|| vec![0.0; spec.number_of_bins]);
            let mut vdw = energy_vdw.unwrap_or_else(|| vec![0.0; spec.number_of_bins]);
            let mut coulomb = energy_coulomb.unwrap_or_else(|| vec![0.0; spec.number_of_bins]);
            let polarization =
                energy_polarization.unwrap_or_else(|| vec![0.0; spec.number_of_bins]);
            let norm = total.iter().sum::<f64>().max(1.0);
            for value in &mut total {
                *value /= norm;
            }
            for value in &mut vdw {
                *value /= norm;
            }
            for value in &mut coulomb {
                *value /= norm;
            }
            Some(EnergyHistogram {
                total,
                vdw,
                coulomb,
                polarization,
            })
        } else {
            None
        };

        let number_histogram = if let (Some(_spec), Some(mut per_component)) =
            (&request.number_histogram, number_histogram)
        {
            let norm = per_component[0].iter().sum::<f64>().max(1.0);
            for value in &mut per_component[0] {
                *value /= norm;
            }
            Some(NumberHistogram { per_component })
        } else {
            None
        };

        let denom = request.production_cycles.max(1) as f64;
        Ok(GcmcResult {
            summary: GcmcSummary {
                attempted_cycles: total_cycles,
                accepted_insertions,
                accepted_deletions,
                accepted_translations,
                accepted_rotations,
                accepted_reinsertions,
                accepted_widom,
                rejected_moves,
                mean_loading: production_loading_sum / denom,
                mean_energy: production_energy_sum / denom,
            },
            final_candidate: current_candidate,
            density_grid,
            energy_histogram,
            number_histogram,
            production_trace,
        })
    }
}

fn validate_gcmc_request(request: &GcmcRequest) -> Result<(), RaspaInterfaceError> {
    if request.guest_template.atoms.is_empty() {
        return Err(RaspaInterfaceError::InvalidGuestTemplate);
    }
    if !request.conditions.temperature_kelvin.is_finite()
        || request.conditions.temperature_kelvin <= 0.0
    {
        return Err(RaspaInterfaceError::InvalidTemperature);
    }
    if request.max_guest_count == 0 {
        return Err(RaspaInterfaceError::InvalidMaximumGuestCount);
    }
    let has_mu = request.conditions.chemical_potential.is_some();
    let has_pressure = request
        .conditions
        .fugacity_pascal
        .or(request.conditions.pressure_pascal)
        .map(|value| value.is_finite() && value > 0.0)
        .unwrap_or(false);
    if !has_mu && !has_pressure {
        return Err(RaspaInterfaceError::MissingGrandCanonicalControl);
    }
    Ok(())
}

fn grand_canonical_activity(conditions: &GcmcConditions) -> Result<f64, RaspaInterfaceError> {
    if let Some(mu) = conditions.chemical_potential {
        let beta = 1.0 / (8.617_333_262_145e-5 * conditions.temperature_kelvin);
        return Ok((beta * mu).exp());
    }
    let pressure = conditions
        .fugacity_pascal
        .or(conditions.pressure_pascal)
        .ok_or(RaspaInterfaceError::MissingGrandCanonicalControl)?;
    if !pressure.is_finite() || pressure <= 0.0 {
        return Err(RaspaInterfaceError::InvalidPressure);
    }
    let activity = pressure / (1.380_649e-23 * conditions.temperature_kelvin) * 1.0e-30;
    Ok(activity)
}

#[derive(Debug, Clone)]
enum GcmcMove {
    Translation,
    Rotation,
    Reinsertion,
    SwapInsert,
    SwapDelete,
    Widom,
}

impl GcmcMove {
    fn as_str(&self) -> &'static str {
        match self {
            Self::Translation => "translation",
            Self::Rotation => "rotation",
            Self::Reinsertion => "reinsertion",
            Self::SwapInsert => "swap_insert",
            Self::SwapDelete => "swap_delete",
            Self::Widom => "widom",
        }
    }
}

fn choose_move(schedule: &GcmcMoveSchedule, guest_count: usize, rng: &mut Lcg64) -> GcmcMove {
    let mut options = Vec::<(GcmcMove, f64)>::new();
    if guest_count > 0 {
        options.push((
            GcmcMove::Translation,
            schedule.translation_probability.max(0.0),
        ));
        options.push((GcmcMove::Rotation, schedule.rotation_probability.max(0.0)));
        options.push((
            GcmcMove::Reinsertion,
            schedule.reinsertion_probability.max(0.0),
        ));
        options.push((
            GcmcMove::SwapDelete,
            schedule.swap_probability.max(0.0) * 0.5,
        ));
    } else {
        options.push((GcmcMove::SwapInsert, schedule.swap_probability.max(0.0)));
    }
    if schedule.swap_probability > 0.0 && guest_count > 0 {
        options.push((
            GcmcMove::SwapInsert,
            schedule.swap_probability.max(0.0) * 0.5,
        ));
    }
    if schedule.widom_probability > 0.0 {
        options.push((GcmcMove::Widom, schedule.widom_probability.max(0.0)));
    }
    let total_weight = options.iter().map(|(_, weight)| *weight).sum::<f64>();
    if total_weight <= 0.0 {
        return if guest_count == 0 {
            GcmcMove::SwapInsert
        } else {
            GcmcMove::Translation
        };
    }
    let mut threshold = rng.uniform() * total_weight;
    for (choice, weight) in options {
        threshold -= weight;
        if threshold <= 0.0 {
            return choice;
        }
    }
    GcmcMove::Translation
}

fn propose_move(
    move_choice: &GcmcMove,
    placements: &[GuestPlacement],
    max_guest_count: usize,
    rng: &mut Lcg64,
) -> Option<(Vec<GuestPlacement>, isize)> {
    let mut next = placements.to_vec();
    match move_choice {
        GcmcMove::SwapInsert | GcmcMove::Widom => {
            if placements.len() >= max_guest_count {
                return None;
            }
            next.push(random_guest_placement(rng));
            Some((next, 1))
        }
        GcmcMove::SwapDelete => {
            if placements.is_empty() {
                return None;
            }
            next.remove(rng.range_usize(next.len()));
            Some((next, -1))
        }
        GcmcMove::Translation => {
            if placements.is_empty() {
                return None;
            }
            let index = rng.range_usize(next.len());
            let mut placement = next[index].clone();
            for axis in &mut placement.center_fractional {
                *axis = (*axis + (rng.uniform() - 0.5) * 0.20).rem_euclid(1.0);
            }
            next[index] = placement;
            Some((next, 0))
        }
        GcmcMove::Rotation => {
            if placements.is_empty() {
                return None;
            }
            let index = rng.range_usize(next.len());
            next[index].rotation = random_rotation(rng);
            Some((next, 0))
        }
        GcmcMove::Reinsertion => {
            if placements.is_empty() {
                return None;
            }
            let index = rng.range_usize(next.len());
            next[index] = random_guest_placement(rng);
            Some((next, 0))
        }
    }
}

fn random_guest_placement(rng: &mut Lcg64) -> GuestPlacement {
    GuestPlacement {
        center_fractional: [rng.uniform(), rng.uniform(), rng.uniform()],
        rotation: random_rotation(rng),
    }
}

fn random_rotation(rng: &mut Lcg64) -> [[f64; 3]; 3] {
    let u1 = rng.uniform();
    let u2 = rng.uniform();
    let u3 = rng.uniform();
    let q1 = (1.0 - u1).sqrt() * (2.0 * std::f64::consts::PI * u2).sin();
    let q2 = (1.0 - u1).sqrt() * (2.0 * std::f64::consts::PI * u2).cos();
    let q3 = u1.sqrt() * (2.0 * std::f64::consts::PI * u3).sin();
    let q4 = u1.sqrt() * (2.0 * std::f64::consts::PI * u3).cos();
    [
        [
            1.0 - 2.0 * (q3 * q3 + q4 * q4),
            2.0 * (q2 * q3 - q1 * q4),
            2.0 * (q2 * q4 + q1 * q3),
        ],
        [
            2.0 * (q2 * q3 + q1 * q4),
            1.0 - 2.0 * (q2 * q2 + q4 * q4),
            2.0 * (q3 * q4 - q1 * q2),
        ],
        [
            2.0 * (q2 * q4 - q1 * q3),
            2.0 * (q3 * q4 + q1 * q2),
            1.0 - 2.0 * (q2 * q2 + q3 * q3),
        ],
    ]
}

fn acceptance_probability(
    move_choice: &GcmcMove,
    beta: f64,
    delta_energy: f64,
    activity: f64,
    cell_volume_angstrom3: f64,
    guest_count: usize,
    delta_particles: isize,
) -> f64 {
    let boltzmann = (-beta * delta_energy).exp();
    let probability = match move_choice {
        GcmcMove::SwapInsert | GcmcMove::Widom if delta_particles > 0 => {
            activity * cell_volume_angstrom3 / (guest_count as f64 + 1.0) * boltzmann
        }
        GcmcMove::SwapDelete if delta_particles < 0 => {
            guest_count as f64 / (activity * cell_volume_angstrom3).max(1.0e-30) * boltzmann
        }
        _ => boltzmann,
    };
    probability.clamp(0.0, 1.0)
}

fn candidate_respects_minimum_distances(
    candidate: &Candidate,
    host_atom_count: usize,
    guest_atom_count: usize,
    minimum_guest_host_distance: f64,
    minimum_guest_guest_distance: f64,
) -> bool {
    let Some(lattice) = candidate.lattice else {
        return false;
    };
    for guest_index in host_atom_count..candidate.len() {
        for host_index in 0..host_atom_count {
            let distance = minimum_image_distance(
                lattice,
                candidate.fractional_coords[guest_index],
                candidate.fractional_coords[host_index],
            );
            if distance < minimum_guest_host_distance {
                return false;
            }
        }
    }
    for left in host_atom_count..candidate.len() {
        for right in (left + 1)..candidate.len() {
            let left_molecule = (left - host_atom_count) / guest_atom_count.max(1);
            let right_molecule = (right - host_atom_count) / guest_atom_count.max(1);
            if left_molecule == right_molecule {
                continue;
            }
            let distance = minimum_image_distance(
                lattice,
                candidate.fractional_coords[left],
                candidate.fractional_coords[right],
            );
            if distance < minimum_guest_guest_distance {
                return false;
            }
        }
    }
    true
}

fn build_guest_configuration_candidate(
    framework: &PeriodicFramework,
    placements: &[GuestPlacement],
    guest_template: &RigidGuestTemplate,
) -> Result<Candidate, RaspaInterfaceError> {
    let mut candidate = framework.to_candidate();
    candidate.label = format!("{}__{}", framework.label, guest_template.label);
    for placement in placements {
        let center_cartesian =
            fractional_to_cartesian(framework.lattice, placement.center_fractional);
        for atom in &guest_template.atoms {
            let rotated = multiply_rotation_vector(placement.rotation, atom.cartesian);
            let cartesian = [
                center_cartesian[0] + rotated[0],
                center_cartesian[1] + rotated[1],
                center_cartesian[2] + rotated[2],
            ];
            let fractional =
                cartesian_to_fractional(framework.lattice, cartesian).ok_or_else(|| {
                    RaspaInterfaceError::InvalidCandidate("framework lattice is singular".into())
                })?;
            let fractional = normalize_fractional_coordinate(fractional);
            candidate.species.push(atom.species.clone());
            candidate.fractional_coords.push(fractional);
        }
    }
    Ok(candidate)
}

fn minimum_image_distance(lattice: [[f64; 3]; 3], left: [f64; 3], right: [f64; 3]) -> f64 {
    let left_cartesian = fractional_to_cartesian(lattice, left);
    let right_cartesian = fractional_to_cartesian(lattice, right);
    minimum_image_cartesian_distance_sq(left_cartesian, right_cartesian, Some(lattice)).sqrt()
}

fn multiply_rotation_vector(rotation: [[f64; 3]; 3], cartesian: [f64; 3]) -> [f64; 3] {
    [
        rotation[0][0] * cartesian[0]
            + rotation[0][1] * cartesian[1]
            + rotation[0][2] * cartesian[2],
        rotation[1][0] * cartesian[0]
            + rotation[1][1] * cartesian[1]
            + rotation[1][2] * cartesian[2],
        rotation[2][0] * cartesian[0]
            + rotation[2][1] * cartesian[1]
            + rotation[2][2] * cartesian[2],
    ]
}
