use crate::structure::{
    centre_of_mass, centroid, passes_min_distance, principal_axis_pca, ClusterStructure,
};
use crate::{ClusterPerturbationEngine, PerturberError};
use nalgebra::Vector3;
use rand::{distributions::Uniform, rngs::StdRng, Rng, SeedableRng};
use rand_distr::{Distribution, Normal};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum PerturbDistribution {
    Gaussian,
    Laplace,
    Uniform,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum PerturbMode {
    S,
    Sp,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum PerturbAxis {
    Auto,
    X,
    Y,
    Z,
    Pca,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum PerturbCentre {
    Com,
    Origin,
    None,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct PerturbationConfig {
    pub sigma: f64,
    pub max_displacement: Option<f64>,
    pub validate_min_distance: Option<f64>,
    pub seed: Option<u64>,
    pub dist: PerturbDistribution,
    pub mode: PerturbMode,
    pub anisotropy: f64,
    pub axis: PerturbAxis,
    pub centre: PerturbCentre,
    pub max_attempts: usize,
    pub parallel: bool,
}

impl Default for PerturbationConfig {
    fn default() -> Self {
        Self {
            sigma: 0.03,
            max_displacement: Some(0.10),
            validate_min_distance: None,
            seed: None,
            dist: PerturbDistribution::Gaussian,
            mode: PerturbMode::S,
            anisotropy: 2.5,
            axis: PerturbAxis::Auto,
            centre: PerturbCentre::Com,
            max_attempts: 200,
            parallel: false,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PerturbationBatch {
    pub source: ClusterStructure,
    pub variants: Vec<ClusterStructure>,
}

#[derive(Debug, Clone, Default)]
pub struct DefaultClusterPerturbationEngine;

impl ClusterPerturbationEngine for DefaultClusterPerturbationEngine {
    fn generate(
        &self,
        source: &ClusterStructure,
        config: PerturbationConfig,
        count: usize,
    ) -> Result<PerturbationBatch, PerturberError> {
        source.validate()?;
        validate_config(config, count)?;

        let mut rng = StdRng::seed_from_u64(config.seed.unwrap_or(0x5eed_u64));
        let centered = recenter_structure(source, config.centre);
        let basis = if config.mode == PerturbMode::Sp {
            Some(SpBasis::from_structure(&centered, config.axis)?)
        } else {
            None
        };
        let mut sampler = ComponentSampler::new(config.dist, config.sigma)
            .map_err(|_| PerturberError::InvalidSigma(config.sigma))?;

        let mut variants = Vec::with_capacity(count);
        'outer: for index in 0..count {
            let mut attempts = 0usize;
            loop {
                attempts += 1;
                let mut variant = centered.clone();
                variant.label = format!("{}__perturbed_{index:04}", source.label);
                for atom in &mut variant.atoms {
                    let mut displacement = match config.mode {
                        PerturbMode::S => sampler.sample_vec3(&mut rng),
                        PerturbMode::Sp => {
                            let basis = basis.as_ref().expect("sp basis");
                            let d_par = sampler.sample_scalar_scaled(&mut rng, config.anisotropy);
                            let d_perp1 = sampler.sample_scalar(&mut rng);
                            let d_perp2 = sampler.sample_scalar(&mut rng);
                            basis.u * d_par + basis.v * d_perp1 + basis.w * d_perp2
                        }
                    };

                    if let Some(max_displacement) = config.max_displacement {
                        let norm = displacement.norm();
                        if max_displacement > 0.0 && norm > max_displacement && norm > 0.0 {
                            displacement *= max_displacement / norm;
                        }
                    }

                    atom.cartesian[0] += displacement.x;
                    atom.cartesian[1] += displacement.y;
                    atom.cartesian[2] += displacement.z;
                }

                if let Some(dmin) = config.validate_min_distance {
                    if !passes_min_distance(&variant, dmin) {
                        if attempts >= config.max_attempts {
                            return Err(PerturberError::MinDistanceViolation { attempts });
                        }
                        continue;
                    }
                }

                variants.push(restore_centre(variant, config.centre, source));
                continue 'outer;
            }
        }

        Ok(PerturbationBatch {
            source: source.clone(),
            variants,
        })
    }
}

fn validate_config(config: PerturbationConfig, count: usize) -> Result<(), PerturberError> {
    if !config.sigma.is_finite() || config.sigma <= 0.0 {
        return Err(PerturberError::InvalidSigma(config.sigma));
    }
    if count == 0 {
        return Err(PerturberError::InvalidCount(count));
    }
    if config.mode == PerturbMode::Sp
        && (!config.anisotropy.is_finite() || config.anisotropy <= 0.0)
    {
        return Err(PerturberError::InvalidCandidate(
            "anisotropy must be finite and > 0 for sp mode".into(),
        ));
    }
    if let Some(dmin) = config.validate_min_distance {
        if !dmin.is_finite() || dmin < 0.0 {
            return Err(PerturberError::InvalidCandidate(
                "validate_min_distance must be finite and >= 0".into(),
            ));
        }
    }
    if config.max_attempts == 0 {
        return Err(PerturberError::InvalidCandidate(
            "max_attempts must be > 0".into(),
        ));
    }
    Ok(())
}

#[derive(Debug, Clone)]
struct SpBasis {
    u: Vector3<f64>,
    v: Vector3<f64>,
    w: Vector3<f64>,
}

impl SpBasis {
    fn from_structure(
        structure: &ClusterStructure,
        axis: PerturbAxis,
    ) -> Result<Self, PerturberError> {
        let u = match axis {
            PerturbAxis::X => Vector3::new(1.0, 0.0, 0.0),
            PerturbAxis::Y => Vector3::new(0.0, 1.0, 0.0),
            PerturbAxis::Z => Vector3::new(0.0, 0.0, 1.0),
            PerturbAxis::Pca | PerturbAxis::Auto => principal_axis_pca(structure)?,
        };
        let u = normalize_or_fallback(u, Vector3::new(1.0, 0.0, 0.0));
        let (v, w) = orthonormal_complement(u);
        Ok(Self { u, v, w })
    }
}

fn normalize_or_fallback(value: Vector3<f64>, fallback: Vector3<f64>) -> Vector3<f64> {
    let norm = value.norm();
    if norm > 0.0 && norm.is_finite() {
        value / norm
    } else {
        fallback
    }
}

fn orthonormal_complement(u: Vector3<f64>) -> (Vector3<f64>, Vector3<f64>) {
    let anchor = if u.x.abs() < 0.9 {
        Vector3::new(1.0, 0.0, 0.0)
    } else {
        Vector3::new(0.0, 1.0, 0.0)
    };
    let v = normalize_or_fallback(u.cross(&anchor), Vector3::new(0.0, 0.0, 1.0));
    let w = normalize_or_fallback(u.cross(&v), Vector3::new(0.0, 1.0, 0.0));
    (v, w)
}

fn recenter_structure(source: &ClusterStructure, centre: PerturbCentre) -> ClusterStructure {
    match centre {
        PerturbCentre::None => source.clone(),
        PerturbCentre::Origin | PerturbCentre::Com => {
            let shift = match centre {
                PerturbCentre::Origin => centroid(source),
                PerturbCentre::Com => centre_of_mass(source),
                PerturbCentre::None => Vector3::new(0.0, 0.0, 0.0),
            };
            let mut centered = source.clone();
            for atom in &mut centered.atoms {
                atom.cartesian[0] -= shift.x;
                atom.cartesian[1] -= shift.y;
                atom.cartesian[2] -= shift.z;
            }
            centered
        }
    }
}

fn restore_centre(
    mut variant: ClusterStructure,
    centre: PerturbCentre,
    source: &ClusterStructure,
) -> ClusterStructure {
    match centre {
        PerturbCentre::None => variant,
        PerturbCentre::Origin | PerturbCentre::Com => {
            let shift = match centre {
                PerturbCentre::Origin => centroid(source),
                PerturbCentre::Com => centre_of_mass(source),
                PerturbCentre::None => Vector3::new(0.0, 0.0, 0.0),
            };
            for atom in &mut variant.atoms {
                atom.cartesian[0] += shift.x;
                atom.cartesian[1] += shift.y;
                atom.cartesian[2] += shift.z;
            }
            variant
        }
    }
}

struct ComponentSampler {
    dist: PerturbDistribution,
    sigma: f64,
    normal: Normal<f64>,
    uniform: Uniform<f64>,
}

impl ComponentSampler {
    fn new(dist: PerturbDistribution, sigma: f64) -> Result<Self, rand_distr::NormalError> {
        let normal = Normal::new(0.0, sigma)?;
        let a = sigma * 3.0_f64.sqrt();
        let uniform = Uniform::new_inclusive(-a, a);
        Ok(Self {
            dist,
            sigma,
            normal,
            uniform,
        })
    }

    fn sample_scalar<RngT: Rng + ?Sized>(&mut self, rng: &mut RngT) -> f64 {
        match self.dist {
            PerturbDistribution::Gaussian => self.normal.sample(rng),
            PerturbDistribution::Uniform => self.uniform.sample(rng),
            PerturbDistribution::Laplace => sample_laplace(rng, self.sigma),
        }
    }

    fn sample_scalar_scaled<RngT: Rng + ?Sized>(&mut self, rng: &mut RngT, scale: f64) -> f64 {
        self.sample_scalar(rng) * scale
    }

    fn sample_vec3<RngT: Rng + ?Sized>(&mut self, rng: &mut RngT) -> Vector3<f64> {
        Vector3::new(
            self.sample_scalar(rng),
            self.sample_scalar(rng),
            self.sample_scalar(rng),
        )
    }
}

fn sample_laplace<RngT: Rng + ?Sized>(rng: &mut RngT, sigma: f64) -> f64 {
    let b = sigma / 2.0_f64.sqrt();
    let u: f64 = rng.gen_range(-0.5..0.5);
    let sign = if u < 0.0 { -1.0 } else { 1.0 };
    let inner = 1.0 - 2.0 * u.abs();
    sign * b * inner.ln()
}

#[cfg(test)]
mod tests {
    use super::{recenter_structure, restore_centre, PerturbCentre};
    use crate::structure::{centre_of_mass, centroid, ClusterAtom, ClusterStructure};

    fn hetero_cluster() -> ClusterStructure {
        ClusterStructure {
            label: "hetero".into(),
            atoms: vec![
                ClusterAtom {
                    species: "Mg".into(),
                    cartesian: [0.0, 0.0, 0.0],
                },
                ClusterAtom {
                    species: "O".into(),
                    cartesian: [10.0, 0.0, 0.0],
                },
            ],
        }
    }

    #[test]
    fn recenter_origin_uses_centroid() {
        let structure = hetero_cluster();
        let centered = recenter_structure(&structure, PerturbCentre::Origin);
        let centered_centroid = centroid(&centered);

        assert!(centered_centroid.norm() < 1.0e-12);
        assert!((centered.atoms[0].cartesian[0] + 5.0).abs() < 1.0e-12);
        assert!((centered.atoms[1].cartesian[0] - 5.0).abs() < 1.0e-12);
    }

    #[test]
    fn recenter_com_uses_mass_weighted_shift() {
        let structure = hetero_cluster();
        let centered = recenter_structure(&structure, PerturbCentre::Com);
        let centered_com = centre_of_mass(&centered);
        let centered_centroid = centroid(&centered);

        assert!(centered_com.norm() < 1.0e-12);
        assert!(centered_centroid.x > 0.0);
        assert!((centered.atoms[0].cartesian[0] + 3.969_581_183_009_130_8).abs() < 1.0e-12);
        assert!((centered.atoms[1].cartesian[0] - 6.030_418_816_990_869).abs() < 1.0e-12);
    }

    #[test]
    fn restore_centre_roundtrips_com_centering() {
        let structure = hetero_cluster();
        let centered = recenter_structure(&structure, PerturbCentre::Com);
        let restored = restore_centre(centered, PerturbCentre::Com, &structure);

        assert_eq!(restored, structure);
    }
}
