use std::collections::BTreeMap;

use patina_sci_kernel::fractional_to_cartesian;
use serde::{Deserialize, Serialize};

use crate::framework::{species_catalog, PeriodicFramework};
use crate::RaspaInterfaceError;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DensityGridNormalization {
    Max,
    NumberDensity,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DensityGridBinning {
    Standard,
    Equitable,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DensityGridSpec {
    pub dimensions: [usize; 3],
    pub sample_every: usize,
    pub write_every: usize,
    pub normalization: DensityGridNormalization,
    pub binning: DensityGridBinning,
    pub pseudo_atom_channels: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DensityGrid {
    pub dimensions: [usize; 3],
    pub channels: usize,
    pub samples: usize,
    pub values: Vec<f64>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EnergyHistogramSpec {
    pub number_of_bins: usize,
    pub range: (f64, f64),
    pub sample_every: usize,
    pub write_every: usize,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EnergyHistogram {
    pub total: Vec<f64>,
    pub vdw: Vec<f64>,
    pub coulomb: Vec<f64>,
    pub polarization: Vec<f64>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct NumberHistogramSpec {
    pub lower_limit: usize,
    pub upper_limit: usize,
    pub sample_every: usize,
    pub write_every: usize,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct NumberHistogram {
    pub per_component: Vec<Vec<f64>>,
}

pub trait PropertyGridEngine {
    fn sample_density_grid(
        &self,
        framework: &PeriodicFramework,
        spec: &DensityGridSpec,
    ) -> Result<DensityGrid, RaspaInterfaceError>;
}

pub trait HistogramEngine {
    fn energy_histogram(
        &self,
        framework: &PeriodicFramework,
        spec: &EnergyHistogramSpec,
    ) -> Result<EnergyHistogram, RaspaInterfaceError>;

    fn number_histogram(
        &self,
        framework: &PeriodicFramework,
        spec: &NumberHistogramSpec,
    ) -> Result<NumberHistogram, RaspaInterfaceError>;
}

#[derive(Debug, Clone, Default)]
pub struct StaticFrameworkPropertyEngine;

impl PropertyGridEngine for StaticFrameworkPropertyEngine {
    fn sample_density_grid(
        &self,
        framework: &PeriodicFramework,
        spec: &DensityGridSpec,
    ) -> Result<DensityGrid, RaspaInterfaceError> {
        if spec.dimensions.contains(&0) {
            return Err(RaspaInterfaceError::InvalidDensityGridDimensions);
        }
        let channels = if spec.pseudo_atom_channels.is_empty() {
            species_catalog(framework)
        } else {
            spec.pseudo_atom_channels.clone()
        };
        let nx = spec.dimensions[0];
        let ny = spec.dimensions[1];
        let nz = spec.dimensions[2];
        let total_voxels = nx * ny * nz;
        let mut values = vec![0.0; channels.len() * total_voxels];

        let channel_map = channels
            .iter()
            .enumerate()
            .map(|(index, label)| (label.as_str(), index))
            .collect::<BTreeMap<_, _>>();

        for atom in &framework.atoms {
            let channel = channel_map.get(atom.species.trim()).copied();
            let Some(channel) = channel else { continue };
            match spec.binning {
                DensityGridBinning::Standard => {
                    let ix = wrapped_grid_index(atom.fractional[0], nx);
                    let iy = wrapped_grid_index(atom.fractional[1], ny);
                    let iz = wrapped_grid_index(atom.fractional[2], nz);
                    let offset = ((channel * nx + ix) * ny + iy) * nz + iz;
                    values[offset] += 1.0;
                }
                DensityGridBinning::Equitable => {
                    deposit_equitable(
                        &mut values,
                        &spec.dimensions,
                        channel,
                        atom.fractional,
                        channels.len(),
                    );
                }
            }
        }

        match spec.normalization {
            DensityGridNormalization::Max => {
                for channel in 0..channels.len() {
                    let start = channel * total_voxels;
                    let end = start + total_voxels;
                    let max_value = values[start..end].iter().copied().fold(0.0_f64, f64::max);
                    if max_value > 0.0 {
                        for value in &mut values[start..end] {
                            *value /= max_value;
                        }
                    }
                }
            }
            DensityGridNormalization::NumberDensity => {
                let cell_volume = cell_volume(framework.lattice).abs();
                let normalization = if framework.atom_count() > 0 && cell_volume > 0.0 {
                    total_voxels as f64 / (cell_volume * framework.atom_count() as f64)
                } else {
                    1.0
                };
                for value in &mut values {
                    *value *= normalization;
                }
            }
        }

        Ok(DensityGrid {
            dimensions: spec.dimensions,
            channels: channels.len(),
            samples: framework.atom_count(),
            values,
        })
    }
}

impl HistogramEngine for StaticFrameworkPropertyEngine {
    fn energy_histogram(
        &self,
        framework: &PeriodicFramework,
        spec: &EnergyHistogramSpec,
    ) -> Result<EnergyHistogram, RaspaInterfaceError> {
        if spec.number_of_bins == 0 {
            return Err(RaspaInterfaceError::InvalidEnergyHistogramBins);
        }
        if !spec.range.0.is_finite() || !spec.range.1.is_finite() || spec.range.1 <= spec.range.0 {
            return Err(RaspaInterfaceError::InvalidEnergyHistogramRange);
        }

        let sample_resolution = (spec.number_of_bins as f64).cbrt().ceil().max(4.0) as usize;
        let mut total = vec![0.0; spec.number_of_bins];
        let mut vdw = vec![0.0; spec.number_of_bins];
        let mut coulomb = vec![0.0; spec.number_of_bins];
        let polarization = vec![0.0; spec.number_of_bins];
        let cell = framework.lattice;

        for ix in 0..sample_resolution {
            for iy in 0..sample_resolution {
                for iz in 0..sample_resolution {
                    let fractional = [
                        (ix as f64 + 0.5) / sample_resolution as f64,
                        (iy as f64 + 0.5) / sample_resolution as f64,
                        (iz as f64 + 0.5) / sample_resolution as f64,
                    ];
                    let (vdw_energy, coulomb_energy) =
                        probe_energy_components(framework, cell, fractional);
                    let total_energy = vdw_energy + coulomb_energy;
                    increment_histogram(&mut total, total_energy, spec.range);
                    increment_histogram(&mut vdw, vdw_energy, spec.range);
                    increment_histogram(&mut coulomb, coulomb_energy, spec.range);
                }
            }
        }

        Ok(EnergyHistogram {
            total,
            vdw,
            coulomb,
            polarization,
        })
    }

    fn number_histogram(
        &self,
        framework: &PeriodicFramework,
        spec: &NumberHistogramSpec,
    ) -> Result<NumberHistogram, RaspaInterfaceError> {
        if spec.upper_limit < spec.lower_limit {
            return Err(RaspaInterfaceError::InvalidNumberHistogramRange);
        }
        let species = species_catalog(framework);
        let width = spec.upper_limit - spec.lower_limit + 1;
        let counts =
            framework
                .atoms
                .iter()
                .fold(BTreeMap::<String, usize>::new(), |mut acc, atom| {
                    *acc.entry(atom.species.trim().to_string()).or_default() += 1;
                    acc
                });
        let per_component = species
            .into_iter()
            .map(|species| {
                let mut bins = vec![0.0; width];
                let count = counts.get(&species).copied().unwrap_or(0);
                if count >= spec.lower_limit && count <= spec.upper_limit {
                    bins[count - spec.lower_limit] = 1.0;
                }
                bins
            })
            .collect();
        Ok(NumberHistogram { per_component })
    }
}

pub(crate) fn wrapped_grid_index(value: f64, size: usize) -> usize {
    let fractional = value.rem_euclid(1.0);
    let index = (fractional * size as f64).floor() as usize;
    index.min(size.saturating_sub(1))
}

pub(crate) fn deposit_equitable(
    values: &mut [f64],
    dimensions: &[usize; 3],
    channel: usize,
    fractional: [f64; 3],
    channel_count: usize,
) {
    let [nx, ny, nz] = *dimensions;
    let axes = [nx, ny, nz]
        .into_iter()
        .zip(fractional)
        .map(|(size, coordinate)| {
            let wrapped = coordinate.rem_euclid(1.0) * size as f64;
            let left = wrapped.floor() as usize % size;
            let right = (left + 1) % size;
            let w_right = wrapped.fract();
            let w_left = 1.0 - w_right;
            (left, right, w_left, w_right)
        })
        .collect::<Vec<_>>();

    for &(ix, wx) in &[(axes[0].0, axes[0].2), (axes[0].1, axes[0].3)] {
        for &(iy, wy) in &[(axes[1].0, axes[1].2), (axes[1].1, axes[1].3)] {
            for &(iz, wz) in &[(axes[2].0, axes[2].2), (axes[2].1, axes[2].3)] {
                let offset = ((channel * nx + ix) * ny + iy) * nz + iz;
                values[offset] += wx * wy * wz;
            }
        }
    }

    let _ = channel_count;
}

pub(crate) fn cell_volume(lattice: [[f64; 3]; 3]) -> f64 {
    let a = lattice[0];
    let b = lattice[1];
    let c = lattice[2];
    dot(cross(a, b), c)
}

fn cross(left: [f64; 3], right: [f64; 3]) -> [f64; 3] {
    [
        left[1] * right[2] - left[2] * right[1],
        left[2] * right[0] - left[0] * right[2],
        left[0] * right[1] - left[1] * right[0],
    ]
}

fn dot(left: [f64; 3], right: [f64; 3]) -> f64 {
    left[0] * right[0] + left[1] * right[1] + left[2] * right[2]
}

fn minimum_image_delta(lattice: [[f64; 3]; 3], left: [f64; 3], right: [f64; 3]) -> [f64; 3] {
    let nearest = [left[0] - right[0], left[1] - right[1], left[2] - right[2]];
    fractional_to_cartesian(
        lattice,
        [
            nearest[0] - nearest[0].round(),
            nearest[1] - nearest[1].round(),
            nearest[2] - nearest[2].round(),
        ],
    )
}

fn probe_energy_components(
    framework: &PeriodicFramework,
    lattice: [[f64; 3]; 3],
    probe_fractional: [f64; 3],
) -> (f64, f64) {
    framework
        .atoms
        .iter()
        .fold((0.0, 0.0), |(vdw_acc, coul_acc), atom| {
            let delta = minimum_image_delta(lattice, probe_fractional, atom.fractional);
            let r = dot(delta, delta).sqrt().max(1.0e-6);
            let sigma = species_sigma(&atom.species);
            let epsilon = species_epsilon(&atom.species);
            let sr6 = (sigma / r).powi(6);
            let vdw = 4.0 * epsilon * (sr6 * sr6 - sr6);
            let coulomb = formal_charge_guess(&atom.species).unwrap_or(0.0) / r;
            (vdw_acc + vdw, coul_acc + coulomb)
        })
}

pub(crate) fn increment_histogram(histogram: &mut [f64], value: f64, range: (f64, f64)) {
    let lower = range.0;
    let upper = range.1;
    if value < lower || value > upper {
        return;
    }
    let width = upper - lower;
    let scaled = ((value - lower) / width * histogram.len() as f64).floor() as usize;
    let index = scaled.min(histogram.len().saturating_sub(1));
    histogram[index] += 1.0;
}

fn species_sigma(symbol: &str) -> f64 {
    match symbol.trim() {
        "H" => 2.5,
        "C" => 3.4,
        "N" => 3.3,
        "O" => 3.0,
        "Mg" => 3.2,
        "Si" => 3.8,
        "Zn" => 3.1,
        _ => 3.0,
    }
}

fn species_epsilon(symbol: &str) -> f64 {
    match symbol.trim() {
        "H" => 0.02,
        "C" => 0.08,
        "N" => 0.07,
        "O" => 0.10,
        "Mg" => 0.05,
        "Si" => 0.06,
        "Zn" => 0.05,
        _ => 0.05,
    }
}

fn formal_charge_guess(symbol: &str) -> Option<f64> {
    match symbol.trim() {
        "H" => Some(1.0),
        "Li" | "Na" | "K" => Some(1.0),
        "Mg" | "Ca" | "Sr" | "Ba" | "Zn" | "Cd" => Some(2.0),
        "Al" => Some(3.0),
        "Si" => Some(4.0),
        "O" | "S" | "Se" | "Te" => Some(-2.0),
        "F" | "Cl" | "Br" | "I" => Some(-1.0),
        _ => None,
    }
}
