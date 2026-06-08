use anyhow::{anyhow, bail, Context, Result};
use patina_types::StructureRecord;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::fs;
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::sync::Mutex;
use std::time::{Duration, Instant};

use crate::surrogate_runtime::{
    explicit_environment_path, python_bin_for_environment, uv_project_environment_value,
};
use crate::{FeatureProjectionPort, FeatureRepresentation};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FeatureProjectorKind {
    SimpleStructureStatistics,
    PairDistanceSignature,
    DscribeSoap,
    FeatomicSoap,
}

pub fn build_feature_projector(kind: FeatureProjectorKind) -> Box<dyn FeatureProjectionPort> {
    build_feature_projector_with_python_config(
        kind,
        PythonFeatureProjectorConfig::workspace_default(),
    )
}

pub fn build_feature_projector_with_python_config(
    kind: FeatureProjectorKind,
    python_config: PythonFeatureProjectorConfig,
) -> Box<dyn FeatureProjectionPort> {
    match kind {
        FeatureProjectorKind::SimpleStructureStatistics => {
            Box::new(SimpleStructureStatisticsProjector)
        }
        FeatureProjectorKind::PairDistanceSignature => Box::new(PairDistanceSignatureProjector),
        FeatureProjectorKind::DscribeSoap => Box::new(PythonSoapProjector::new(
            SoapDescriptorProvider::Dscribe,
            python_config,
        )),
        FeatureProjectorKind::FeatomicSoap => Box::new(PythonSoapProjector::new(
            SoapDescriptorProvider::Featomic,
            python_config,
        )),
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PythonFeatureProjectorConfig {
    pub uv_bin: PathBuf,
    pub project_dir: PathBuf,
    pub environment_name: String,
    pub extras: Vec<String>,
    pub native_tls: bool,
    pub timeout: Option<Duration>,
}

impl PythonFeatureProjectorConfig {
    pub fn workspace_default() -> Self {
        Self {
            uv_bin: PathBuf::from("uv"),
            project_dir: PathBuf::from("crates/patina-emulate/python"),
            environment_name: "autoemulate".into(),
            extras: vec!["autoemulate".into()],
            native_tls: true,
            timeout: Some(Duration::from_secs(300)),
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct SimpleStructureStatisticsProjector;

impl SimpleStructureStatisticsProjector {
    pub const FAMILY: &'static str = "simple_structure_statistics";
    pub const VERSION: &'static str = "v1";
    pub const PROVENANCE: &'static str = "patina_emulate.simple_structure_statistics.v1";

    fn base_feature_names() -> Vec<String> {
        vec![
            "atom_count".into(),
            "species_count".into(),
            "periodic_dimension".into(),
            "centroid_x".into(),
            "centroid_y".into(),
            "centroid_z".into(),
            "span_x".into(),
            "span_y".into(),
            "span_z".into(),
            "mean_radius".into(),
            "rms_radius".into(),
            "min_pair_distance".into(),
            "mean_pair_distance".into(),
            "max_pair_distance".into(),
            "lattice_volume".into(),
        ]
    }
}

impl FeatureProjectionPort for SimpleStructureStatisticsProjector {
    fn derive_representation(
        &self,
        structures: &[StructureRecord],
    ) -> Result<FeatureRepresentation> {
        if structures.is_empty() {
            bail!("feature projection requires at least one structure");
        }
        let mut feature_names = Self::base_feature_names();
        for species in basis_species(structures) {
            feature_names.push(format!("species_count:{species}"));
        }
        Ok(FeatureRepresentation {
            family: Self::FAMILY.into(),
            version: Self::VERSION.into(),
            feature_names,
            provenance_label: Self::PROVENANCE.into(),
        })
    }

    fn project_structure(
        &self,
        structure: &StructureRecord,
        representation: &FeatureRepresentation,
    ) -> Result<Vec<f64>> {
        if representation.family != Self::FAMILY || representation.version != Self::VERSION {
            bail!(
                "unsupported feature representation `{}` version `{}`",
                representation.family,
                representation.version
            );
        }
        validate_structure_shape(structure)?;
        let coords = cartesian_coords(structure);
        let atom_count = coords.len();
        let centroid = centroid(&coords);
        let (min_coord, max_coord) = coord_extrema(&coords);
        let radii = radii_from_centroid(&coords, centroid);
        let pair_distances = pair_distances(&coords);
        let species_counts = species_counts(structure);

        let mut features = vec![
            atom_count as f64,
            species_counts.len() as f64,
            structure.periodic_axes.iter().filter(|axis| **axis).count() as f64,
            centroid[0],
            centroid[1],
            centroid[2],
            max_coord[0] - min_coord[0],
            max_coord[1] - min_coord[1],
            max_coord[2] - min_coord[2],
            mean(&radii),
            rms(&radii),
            pair_distances.first().copied().unwrap_or(0.0),
            mean(&pair_distances),
            pair_distances.last().copied().unwrap_or(0.0),
            lattice_volume(structure),
        ];

        for feature_name in representation
            .feature_names
            .iter()
            .skip(Self::base_feature_names().len())
        {
            let species = feature_name
                .strip_prefix("species_count:")
                .ok_or_else(|| anyhow!("unsupported feature name `{feature_name}`"))?;
            features.push(*species_counts.get(species).unwrap_or(&0) as f64);
        }

        if features.len() != representation.feature_names.len() {
            bail!(
                "feature vector length {} does not match representation length {}",
                features.len(),
                representation.feature_names.len()
            );
        }
        Ok(features)
    }
}

#[derive(Debug, Clone, Default)]
pub struct PairDistanceSignatureProjector;

impl PairDistanceSignatureProjector {
    pub const FAMILY: &'static str = "pair_distance_signature";
    pub const VERSION: &'static str = "v1";
    pub const PROVENANCE: &'static str = "patina_emulate.pair_distance_signature.v1";
    const HISTOGRAM_BIN_COUNT: usize = 8;

    fn base_feature_names() -> Vec<String> {
        let mut names = vec![
            "atom_count".into(),
            "species_count".into(),
            "periodic_dimension".into(),
            "span_x".into(),
            "span_y".into(),
            "span_z".into(),
            "mean_radius".into(),
            "rms_radius".into(),
            "nearest_neighbor_min".into(),
            "nearest_neighbor_mean".into(),
            "nearest_neighbor_max".into(),
            "pair_distance_min".into(),
            "pair_distance_mean".into(),
            "pair_distance_std".into(),
            "pair_distance_q10".into(),
            "pair_distance_q25".into(),
            "pair_distance_q50".into(),
            "pair_distance_q75".into(),
            "pair_distance_q90".into(),
            "lattice_volume".into(),
        ];
        for index in 0..Self::HISTOGRAM_BIN_COUNT {
            names.push(format!("pair_distance_hist_bin_{index:02}"));
        }
        names
    }

    fn species_pair_basis(structures: &[StructureRecord]) -> Vec<String> {
        let species = basis_species(structures);
        let mut pairs = Vec::new();
        for left_index in 0..species.len() {
            for right_index in left_index..species.len() {
                pairs.push(species_pair_label(
                    &species[left_index],
                    &species[right_index],
                ));
            }
        }
        pairs
    }
}

impl FeatureProjectionPort for PairDistanceSignatureProjector {
    fn derive_representation(
        &self,
        structures: &[StructureRecord],
    ) -> Result<FeatureRepresentation> {
        if structures.is_empty() {
            bail!("feature projection requires at least one structure");
        }
        let mut feature_names = Self::base_feature_names();
        for species in basis_species(structures) {
            feature_names.push(format!("species_count:{species}"));
        }
        for pair in Self::species_pair_basis(structures) {
            feature_names.push(format!("pair_fraction:{pair}"));
            feature_names.push(format!("pair_mean_distance:{pair}"));
        }
        Ok(FeatureRepresentation {
            family: Self::FAMILY.into(),
            version: Self::VERSION.into(),
            feature_names,
            provenance_label: Self::PROVENANCE.into(),
        })
    }

    fn project_structure(
        &self,
        structure: &StructureRecord,
        representation: &FeatureRepresentation,
    ) -> Result<Vec<f64>> {
        if representation.family != Self::FAMILY || representation.version != Self::VERSION {
            bail!(
                "unsupported feature representation `{}` version `{}`",
                representation.family,
                representation.version
            );
        }
        validate_structure_shape(structure)?;
        let coords = cartesian_coords(structure);
        let atom_count = coords.len();
        let (min_coord, max_coord) = coord_extrema(&coords);
        let centroid = centroid(&coords);
        let radii = radii_from_centroid(&coords, centroid);
        let pair_summary = pair_distance_summary(structure, &coords);
        let species_counts = species_counts(structure);

        let mut features = vec![
            atom_count as f64,
            species_counts.len() as f64,
            structure.periodic_axes.iter().filter(|axis| **axis).count() as f64,
            max_coord[0] - min_coord[0],
            max_coord[1] - min_coord[1],
            max_coord[2] - min_coord[2],
            mean(&radii),
            rms(&radii),
            pair_summary
                .nearest_neighbors
                .first()
                .copied()
                .unwrap_or(0.0),
            mean(&pair_summary.nearest_neighbors),
            pair_summary
                .nearest_neighbors
                .last()
                .copied()
                .unwrap_or(0.0),
            pair_summary.distances.first().copied().unwrap_or(0.0),
            mean(&pair_summary.distances),
            std_dev(&pair_summary.distances),
            quantile(&pair_summary.distances, 0.10),
            quantile(&pair_summary.distances, 0.25),
            quantile(&pair_summary.distances, 0.50),
            quantile(&pair_summary.distances, 0.75),
            quantile(&pair_summary.distances, 0.90),
            lattice_volume(structure),
        ];
        features.extend(normalized_histogram(
            &pair_summary.distances,
            Self::HISTOGRAM_BIN_COUNT,
        ));

        for feature_name in representation
            .feature_names
            .iter()
            .skip(Self::base_feature_names().len())
        {
            if let Some(species) = feature_name.strip_prefix("species_count:") {
                features.push(*species_counts.get(species).unwrap_or(&0) as f64);
                continue;
            }
            if let Some(pair) = feature_name.strip_prefix("pair_fraction:") {
                let pair_count = *pair_summary.species_pair_counts.get(pair).unwrap_or(&0);
                features.push(if pair_summary.total_pairs == 0 {
                    0.0
                } else {
                    pair_count as f64 / pair_summary.total_pairs as f64
                });
                continue;
            }
            if let Some(pair) = feature_name.strip_prefix("pair_mean_distance:") {
                let pair_count = *pair_summary.species_pair_counts.get(pair).unwrap_or(&0);
                let pair_distance_sum = *pair_summary
                    .species_pair_distance_sum
                    .get(pair)
                    .unwrap_or(&0.0);
                features.push(if pair_count == 0 {
                    0.0
                } else {
                    pair_distance_sum / pair_count as f64
                });
                continue;
            }
            bail!("unsupported feature name `{feature_name}`");
        }

        if features.len() != representation.feature_names.len() {
            bail!(
                "feature vector length {} does not match representation length {}",
                features.len(),
                representation.feature_names.len()
            );
        }
        Ok(features)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SoapDescriptorProvider {
    Dscribe,
    Featomic,
}

impl SoapDescriptorProvider {
    fn family(self) -> &'static str {
        match self {
            Self::Dscribe => "dscribe_soap_global",
            Self::Featomic => "featomic_soap_global",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SoapFeatureNormalization {
    None,
    L2,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SoapDescriptorConfig {
    pub cutoff_radius: f64,
    pub smoothing_width: f64,
    pub gaussian_width: f64,
    pub n_max: usize,
    pub l_max: usize,
    pub normalization: SoapFeatureNormalization,
}

impl Default for SoapDescriptorConfig {
    fn default() -> Self {
        Self {
            cutoff_radius: 5.0,
            smoothing_width: 0.5,
            gaussian_width: 0.3,
            n_max: 6,
            l_max: 4,
            normalization: SoapFeatureNormalization::L2,
        }
    }
}

#[derive(Debug)]
pub struct PythonSoapProjector {
    provider: SoapDescriptorProvider,
    descriptor_config: SoapDescriptorConfig,
    python_config: PythonFeatureProjectorConfig,
    cache: Mutex<PythonSoapFeatureCache>,
}

impl PythonSoapProjector {
    pub const VERSION: &'static str = "v1";

    pub fn new(
        provider: SoapDescriptorProvider,
        python_config: PythonFeatureProjectorConfig,
    ) -> Self {
        Self {
            provider,
            descriptor_config: SoapDescriptorConfig::default(),
            python_config,
            cache: Mutex::new(PythonSoapFeatureCache::default()),
        }
    }

    pub fn with_descriptor_config(mut self, descriptor_config: SoapDescriptorConfig) -> Self {
        self.descriptor_config = descriptor_config;
        self
    }

    fn validate_representation(&self, representation: &FeatureRepresentation) -> Result<()> {
        if representation.family != self.provider.family()
            || representation.version != Self::VERSION
        {
            bail!(
                "unsupported feature representation `{}` version `{}` for {:?} SOAP projector",
                representation.family,
                representation.version,
                self.provider
            );
        }
        Ok(())
    }

    fn project_structures(
        &self,
        structures: &[StructureRecord],
    ) -> Result<PythonSoapFeatureResponse> {
        if structures.is_empty() {
            bail!("SOAP feature projection requires at least one structure");
        }
        for structure in structures {
            validate_structure_shape(structure)?;
        }
        let response = run_python_soap_projection(
            &self.python_config,
            self.provider,
            &self.descriptor_config,
            structures,
        )?;
        if response.schema_version != SOAP_FEATURE_RESPONSE_SCHEMA_VERSION {
            bail!(
                "unsupported SOAP feature response schema version `{}`",
                response.schema_version
            );
        }
        if response.family != self.provider.family() {
            bail!(
                "SOAP feature response family `{}` does not match requested provider `{}`",
                response.family,
                self.provider.family()
            );
        }
        if response.version != Self::VERSION {
            bail!(
                "unsupported SOAP feature response version `{}`",
                response.version
            );
        }
        if response.vectors.len() != structures.len() {
            bail!(
                "SOAP feature response vector count {} does not match structure count {}",
                response.vectors.len(),
                structures.len()
            );
        }
        if response.feature_names.is_empty() {
            bail!("SOAP feature response must contain at least one feature name");
        }
        for vector in &response.vectors {
            validate_numeric_vector(
                &vector.features,
                response.feature_names.len(),
                "SOAP feature vector",
            )?;
        }
        Ok(response)
    }
}

impl FeatureProjectionPort for PythonSoapProjector {
    fn derive_representation(
        &self,
        structures: &[StructureRecord],
    ) -> Result<FeatureRepresentation> {
        let response = self.project_structures(structures)?;
        let mut cache = self
            .cache
            .lock()
            .map_err(|_| anyhow!("SOAP feature cache mutex poisoned"))?;
        cache.feature_vectors.clear();
        cache.feature_names = response.feature_names.clone();
        cache.provenance_label = Some(response.provenance_label.clone());
        for (structure, vector) in structures.iter().zip(response.vectors.iter()) {
            cache
                .feature_vectors
                .insert(structure_fingerprint(structure)?, vector.features.clone());
        }

        Ok(FeatureRepresentation {
            family: response.family,
            version: response.version,
            feature_names: response.feature_names,
            provenance_label: response.provenance_label,
        })
    }

    fn project_structure(
        &self,
        structure: &StructureRecord,
        representation: &FeatureRepresentation,
    ) -> Result<Vec<f64>> {
        self.validate_representation(representation)?;
        validate_structure_shape(structure)?;
        let fingerprint = structure_fingerprint(structure)?;
        if let Some(features) = self
            .cache
            .lock()
            .map_err(|_| anyhow!("SOAP feature cache mutex poisoned"))?
            .feature_vectors
            .get(&fingerprint)
            .cloned()
        {
            validate_numeric_vector(
                &features,
                representation.feature_names.len(),
                "cached SOAP feature vector",
            )?;
            return Ok(features);
        }

        let response = self.project_structures(std::slice::from_ref(structure))?;
        let features = response
            .vectors
            .first()
            .ok_or_else(|| anyhow!("SOAP projector returned no vector for structure"))?
            .features
            .clone();
        validate_numeric_vector(
            &features,
            representation.feature_names.len(),
            "SOAP feature vector",
        )?;
        self.cache
            .lock()
            .map_err(|_| anyhow!("SOAP feature cache mutex poisoned"))?
            .feature_vectors
            .insert(fingerprint, features.clone());
        Ok(features)
    }
}

#[derive(Debug, Default)]
struct PythonSoapFeatureCache {
    feature_vectors: BTreeMap<String, Vec<f64>>,
    feature_names: Vec<String>,
    provenance_label: Option<String>,
}

const SOAP_FEATURE_REQUEST_SCHEMA_VERSION: &str = "patina.emulate.feature_projection_request.v1";
const SOAP_FEATURE_RESPONSE_SCHEMA_VERSION: &str = "patina.emulate.feature_projection_response.v1";

#[derive(Debug, Serialize)]
struct PythonSoapFeatureRequest<'a> {
    schema_version: &'static str,
    provider: SoapDescriptorProvider,
    config: &'a SoapDescriptorConfig,
    structures: &'a [StructureRecord],
}

#[derive(Debug, Deserialize)]
struct PythonSoapFeatureResponse {
    schema_version: String,
    family: String,
    version: String,
    feature_names: Vec<String>,
    provenance_label: String,
    vectors: Vec<PythonSoapFeatureVector>,
}

#[derive(Debug, Deserialize)]
struct PythonSoapFeatureVector {
    features: Vec<f64>,
}

#[derive(Debug, Clone, Default)]
struct PairDistanceSummary {
    distances: Vec<f64>,
    nearest_neighbors: Vec<f64>,
    species_pair_counts: BTreeMap<String, usize>,
    species_pair_distance_sum: BTreeMap<String, f64>,
    total_pairs: usize,
}

fn basis_species(structures: &[StructureRecord]) -> Vec<String> {
    let mut species = structures
        .iter()
        .flat_map(|structure| structure.species.iter().cloned())
        .collect::<Vec<_>>();
    species.sort();
    species.dedup();
    species
}

fn validate_structure_shape(structure: &StructureRecord) -> Result<()> {
    if structure.species.len() != structure.fractional_coords.len() {
        bail!(
            "structure `{}` has mismatched species/coordinate lengths",
            structure.label
        );
    }
    if structure.species.is_empty() {
        bail!("structure `{}` contains no sites", structure.label);
    }
    Ok(())
}

fn cartesian_coords(structure: &StructureRecord) -> Vec<[f64; 3]> {
    match structure.lattice {
        Some(lattice) => structure
            .fractional_coords
            .iter()
            .map(|coord| {
                [
                    coord[0] * lattice[0][0] + coord[1] * lattice[1][0] + coord[2] * lattice[2][0],
                    coord[0] * lattice[0][1] + coord[1] * lattice[1][1] + coord[2] * lattice[2][1],
                    coord[0] * lattice[0][2] + coord[1] * lattice[1][2] + coord[2] * lattice[2][2],
                ]
            })
            .collect(),
        None => structure.fractional_coords.clone(),
    }
}

fn lattice_volume(structure: &StructureRecord) -> f64 {
    let Some(lattice) = structure.lattice else {
        return 0.0;
    };
    let a = lattice[0];
    let b = lattice[1];
    let c = lattice[2];
    let cross = [
        b[1] * c[2] - b[2] * c[1],
        b[2] * c[0] - b[0] * c[2],
        b[0] * c[1] - b[1] * c[0],
    ];
    (a[0] * cross[0] + a[1] * cross[1] + a[2] * cross[2]).abs()
}

fn centroid(coords: &[[f64; 3]]) -> [f64; 3] {
    let sum = coords.iter().fold([0.0; 3], |mut acc, coord| {
        acc[0] += coord[0];
        acc[1] += coord[1];
        acc[2] += coord[2];
        acc
    });
    [
        sum[0] / coords.len() as f64,
        sum[1] / coords.len() as f64,
        sum[2] / coords.len() as f64,
    ]
}

fn coord_extrema(coords: &[[f64; 3]]) -> ([f64; 3], [f64; 3]) {
    let mut min_coord = coords[0];
    let mut max_coord = coords[0];
    for coord in coords {
        min_coord[0] = min_coord[0].min(coord[0]);
        min_coord[1] = min_coord[1].min(coord[1]);
        min_coord[2] = min_coord[2].min(coord[2]);
        max_coord[0] = max_coord[0].max(coord[0]);
        max_coord[1] = max_coord[1].max(coord[1]);
        max_coord[2] = max_coord[2].max(coord[2]);
    }
    (min_coord, max_coord)
}

fn radii_from_centroid(coords: &[[f64; 3]], centroid: [f64; 3]) -> Vec<f64> {
    coords
        .iter()
        .map(|coord| {
            let dx = coord[0] - centroid[0];
            let dy = coord[1] - centroid[1];
            let dz = coord[2] - centroid[2];
            (dx * dx + dy * dy + dz * dz).sqrt()
        })
        .collect()
}

fn pair_distances(coords: &[[f64; 3]]) -> Vec<f64> {
    let mut distances = Vec::new();
    for left in 0..coords.len() {
        for right in (left + 1)..coords.len() {
            distances.push(distance(coords[left], coords[right]));
        }
    }
    distances.sort_by(f64::total_cmp);
    distances
}

fn pair_distance_summary(structure: &StructureRecord, coords: &[[f64; 3]]) -> PairDistanceSummary {
    let mut summary = PairDistanceSummary::default();
    if coords.len() == 1 {
        summary.nearest_neighbors.push(0.0);
        return summary;
    }

    let mut nearest_neighbors = vec![f64::INFINITY; coords.len()];
    for left in 0..coords.len() {
        for right in (left + 1)..coords.len() {
            let pair_distance = distance(coords[left], coords[right]);
            summary.distances.push(pair_distance);
            nearest_neighbors[left] = nearest_neighbors[left].min(pair_distance);
            nearest_neighbors[right] = nearest_neighbors[right].min(pair_distance);
            let pair = species_pair_label(&structure.species[left], &structure.species[right]);
            *summary.species_pair_counts.entry(pair.clone()).or_insert(0) += 1;
            *summary.species_pair_distance_sum.entry(pair).or_insert(0.0) += pair_distance;
            summary.total_pairs += 1;
        }
    }
    summary.distances.sort_by(f64::total_cmp);
    nearest_neighbors.sort_by(f64::total_cmp);
    summary.nearest_neighbors = nearest_neighbors;
    summary
}

fn species_counts(structure: &StructureRecord) -> BTreeMap<String, usize> {
    let mut counts = BTreeMap::new();
    for species in &structure.species {
        *counts.entry(species.clone()).or_insert(0) += 1;
    }
    counts
}

fn species_pair_label(left: &str, right: &str) -> String {
    if left <= right {
        format!("{left}-{right}")
    } else {
        format!("{right}-{left}")
    }
}

fn distance(left: [f64; 3], right: [f64; 3]) -> f64 {
    let dx = left[0] - right[0];
    let dy = left[1] - right[1];
    let dz = left[2] - right[2];
    (dx * dx + dy * dy + dz * dz).sqrt()
}

fn mean(values: &[f64]) -> f64 {
    if values.is_empty() {
        0.0
    } else {
        values.iter().sum::<f64>() / values.len() as f64
    }
}

fn rms(values: &[f64]) -> f64 {
    if values.is_empty() {
        0.0
    } else {
        (values.iter().map(|value| value * value).sum::<f64>() / values.len() as f64).sqrt()
    }
}

fn std_dev(values: &[f64]) -> f64 {
    if values.len() <= 1 {
        return 0.0;
    }
    let mean_value = mean(values);
    let variance = values
        .iter()
        .map(|value| {
            let delta = value - mean_value;
            delta * delta
        })
        .sum::<f64>()
        / values.len() as f64;
    variance.sqrt()
}

fn quantile(sorted_values: &[f64], percentile: f64) -> f64 {
    if sorted_values.is_empty() {
        return 0.0;
    }
    if sorted_values.len() == 1 {
        return sorted_values[0];
    }
    let position = percentile.clamp(0.0, 1.0) * (sorted_values.len() - 1) as f64;
    let lower = position.floor() as usize;
    let upper = position.ceil() as usize;
    if lower == upper {
        return sorted_values[lower];
    }
    let weight = position - lower as f64;
    sorted_values[lower] * (1.0 - weight) + sorted_values[upper] * weight
}

fn normalized_histogram(values: &[f64], bin_count: usize) -> Vec<f64> {
    if bin_count == 0 {
        return Vec::new();
    }
    if values.is_empty() {
        return vec![0.0; bin_count];
    }

    let min_value = values.first().copied().unwrap_or(0.0);
    let max_value = values.last().copied().unwrap_or(0.0);
    if (max_value - min_value).abs() <= f64::EPSILON {
        let mut histogram = vec![0.0; bin_count];
        histogram[0] = 1.0;
        return histogram;
    }

    let mut histogram = vec![0usize; bin_count];
    for value in values {
        let normalized = ((value - min_value) / (max_value - min_value)).clamp(0.0, 1.0);
        let index = ((normalized * bin_count as f64).floor() as usize).min(bin_count - 1);
        histogram[index] += 1;
    }

    histogram
        .into_iter()
        .map(|count| count as f64 / values.len() as f64)
        .collect()
}

fn structure_fingerprint(structure: &StructureRecord) -> Result<String> {
    serde_json::to_string(structure).context("failed to serialize structure for feature cache")
}

fn validate_numeric_vector(values: &[f64], expected_len: usize, label: &str) -> Result<()> {
    if values.len() != expected_len {
        bail!(
            "{label} length {} does not match expected length {}",
            values.len(),
            expected_len
        );
    }
    if values.iter().any(|value| !value.is_finite()) {
        bail!("{label} must contain only finite values");
    }
    Ok(())
}

fn run_python_soap_projection(
    python_config: &PythonFeatureProjectorConfig,
    provider: SoapDescriptorProvider,
    descriptor_config: &SoapDescriptorConfig,
    structures: &[StructureRecord],
) -> Result<PythonSoapFeatureResponse> {
    let temp = tempfile::tempdir().context("failed to create SOAP feature tempdir")?;
    let request_path = temp.path().join("feature_request.json");
    let response_path = temp.path().join("feature_response.json");
    let request = PythonSoapFeatureRequest {
        schema_version: SOAP_FEATURE_REQUEST_SCHEMA_VERSION,
        provider,
        config: descriptor_config,
        structures,
    };
    fs::write(
        &request_path,
        serde_json::to_vec_pretty(&request).context("failed to serialize SOAP feature request")?,
    )
    .with_context(|| {
        format!(
            "failed to write SOAP feature request `{}`",
            request_path.display()
        )
    })?;

    let mpl_config_dir = python_config.project_dir.join(".mplcache");
    fs::create_dir_all(&mpl_config_dir).with_context(|| {
        format!(
            "failed to create SOAP projector matplotlib cache dir `{}`",
            mpl_config_dir.display()
        )
    })?;
    let uv_cache_dir = python_config.project_dir.join(".uvcache");
    fs::create_dir_all(&uv_cache_dir).with_context(|| {
        format!(
            "failed to create SOAP projector uv cache dir `{}`",
            uv_cache_dir.display()
        )
    })?;

    let explicit_environment_path =
        explicit_environment_path(&python_config.project_dir, &python_config.environment_name);
    let mut command = if let Some(environment_path) = explicit_environment_path.as_ref() {
        let python_bin = python_bin_for_environment(environment_path).ok_or_else(|| {
            anyhow!(
                "explicit SOAP feature environment `{}` does not contain a runnable python executable",
                environment_path.display()
            )
        })?;
        let mut command = Command::new(python_bin);
        command
            .current_dir(&python_config.project_dir)
            .env_remove("CONDA_PREFIX")
            .env("PYTHONPATH", python_config.project_dir.join("src"))
            .env("MPLCONFIGDIR", &mpl_config_dir)
            .arg("-m")
            .arg("patina_emulate_runtime.cli");
        command
    } else {
        let mut command = Command::new(&python_config.uv_bin);
        command
            .current_dir(&python_config.project_dir)
            .env_remove("CONDA_PREFIX")
            .env(
                "UV_PROJECT_ENVIRONMENT",
                uv_project_environment_value(
                    &python_config.project_dir,
                    &python_config.environment_name,
                ),
            )
            .env("UV_CACHE_DIR", &uv_cache_dir)
            .env("MPLCONFIGDIR", &mpl_config_dir)
            .arg("run")
            .arg("--no-dev");
        if python_config.native_tls {
            command.arg("--native-tls");
        }
        for extra in &python_config.extras {
            command.arg("--extra").arg(extra);
        }
        command
            .arg("--project")
            .arg(&python_config.project_dir)
            .arg("patina-emulate-runtime");
        command
    };

    command
        .arg("project-features")
        .arg("--request")
        .arg(&request_path)
        .arg("--response")
        .arg(&response_path)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());

    let mut child = command
        .spawn()
        .with_context(|| format!("failed to spawn {:?} SOAP feature projector", provider))?;
    let output = if let Some(timeout) = python_config.timeout {
        let started = Instant::now();
        loop {
            if let Some(status) = child
                .try_wait()
                .context("failed to poll SOAP feature projector")?
            {
                let output = child
                    .wait_with_output()
                    .context("failed to collect SOAP feature projector output")?;
                break (status, output.stdout, output.stderr);
            }
            if started.elapsed() > timeout {
                let _ = child.kill();
                let _ = child.wait();
                bail!(
                    "{:?} SOAP feature projector timed out after {:?}",
                    provider,
                    started.elapsed()
                );
            }
            std::thread::sleep(Duration::from_millis(50));
        }
    } else {
        let output = child
            .wait_with_output()
            .context("failed to collect SOAP feature projector output")?;
        (output.status, output.stdout, output.stderr)
    };

    if !output.0.success() {
        let stderr = String::from_utf8_lossy(&output.2);
        bail!(
            "{:?} SOAP feature projector exited with code {:?}: {}",
            provider,
            output.0.code(),
            stderr.trim()
        );
    }
    if !response_path.exists() {
        bail!(
            "{:?} SOAP feature projector did not create `{}`",
            provider,
            response_path.display()
        );
    }
    let raw_response = fs::read(&response_path).with_context(|| {
        format!(
            "failed to read SOAP feature response `{}`",
            response_path.display()
        )
    })?;
    serde_json::from_slice(&raw_response).with_context(|| {
        format!(
            "failed to parse SOAP feature response `{}`",
            response_path.display()
        )
    })
}

#[cfg(test)]
mod tests {
    use super::{
        build_feature_projector, FeatureProjectionPort, FeatureProjectorKind,
        PairDistanceSignatureProjector, SimpleStructureStatisticsProjector,
    };

    fn sample_structure(label: &str) -> patina_types::StructureRecord {
        patina_types::StructureRecord {
            label: label.into(),
            species: vec!["Mg".into(), "O".into()],
            fractional_coords: vec![[0.0, 0.0, 0.0], [0.5, 0.5, 0.5]],
            lattice: None,
            periodic_axes: [false, false, false],
        }
    }

    fn four_atom_structure(label: &str) -> patina_types::StructureRecord {
        patina_types::StructureRecord {
            label: label.into(),
            species: vec!["Mg".into(), "O".into(), "Mg".into(), "O".into()],
            fractional_coords: vec![
                [0.0, 0.0, 0.0],
                [1.0, 0.0, 0.0],
                [0.0, 1.4, 0.0],
                [1.1, 1.2, 0.0],
            ],
            lattice: None,
            periodic_axes: [false, false, false],
        }
    }

    #[test]
    fn projector_derives_species_aware_representation() {
        let projector = SimpleStructureStatisticsProjector;
        let representation = projector
            .derive_representation(&[sample_structure("a"), sample_structure("b")])
            .expect("representation");
        assert_eq!(
            representation.family,
            SimpleStructureStatisticsProjector::FAMILY
        );
        assert!(representation
            .feature_names
            .iter()
            .any(|name| name == "species_count:Mg"));
        assert!(representation
            .feature_names
            .iter()
            .any(|name| name == "species_count:O"));
    }

    #[test]
    fn simple_projector_projects_expected_vector_length() {
        let projector = SimpleStructureStatisticsProjector;
        let representation = projector
            .derive_representation(&[sample_structure("a")])
            .expect("representation");
        let features = projector
            .project_structure(&sample_structure("a"), &representation)
            .expect("feature vector");
        assert_eq!(features.len(), representation.feature_names.len());
        assert_eq!(features[0], 2.0);
    }

    #[test]
    fn pair_distance_projector_derives_pairwise_representation() {
        let projector = PairDistanceSignatureProjector;
        let representation = projector
            .derive_representation(&[four_atom_structure("a"), four_atom_structure("b")])
            .expect("representation");
        assert_eq!(
            representation.family,
            PairDistanceSignatureProjector::FAMILY
        );
        assert!(representation
            .feature_names
            .iter()
            .any(|name| name == "pair_fraction:Mg-O"));
        assert!(representation
            .feature_names
            .iter()
            .any(|name| name == "pair_mean_distance:O-O"));
    }

    #[test]
    fn pair_distance_projector_emits_normalized_histogram_features() {
        let projector = PairDistanceSignatureProjector;
        let representation = projector
            .derive_representation(&[four_atom_structure("a")])
            .expect("representation");
        let features = projector
            .project_structure(&four_atom_structure("a"), &representation)
            .expect("feature vector");
        let histogram_offset = PairDistanceSignatureProjector::base_feature_names().len() - 8;
        let histogram_sum = features[histogram_offset..histogram_offset + 8]
            .iter()
            .sum::<f64>();
        assert!((histogram_sum - 1.0).abs() < 1e-9);
    }

    #[test]
    fn feature_projector_factory_builds_requested_family() {
        let projector = build_feature_projector(FeatureProjectorKind::PairDistanceSignature);
        let representation = projector
            .derive_representation(&[four_atom_structure("factory")])
            .expect("representation");
        assert_eq!(
            representation.family,
            PairDistanceSignatureProjector::FAMILY
        );
    }
}
