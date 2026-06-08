use crate::conversion::{
    ConversionError, ConversionKind, ConversionOutcome, ConversionWarning, InferenceKind,
};
use crate::geometry::{
    fractional_to_cartesian as geometry_fractional_to_cartesian,
    minimum_image_cartesian_distance_sq_with_axes as geometry_minimum_image_cartesian_distance_sq_with_axes,
};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::Path;
use thiserror::Error;

#[cfg(feature = "bridge-patina-types")]
use patina_types::Candidate;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CifDialect {
    Core,
    Mmcif,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CifFindingSeverity {
    Warning,
    Error,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CifFindingCode {
    MissingSymmetryOperations,
    MissingOccupancy,
    OccupancyOutOfRange,
    OccupancySumExceedsOne,
    OverlappingExpandedSites,
    UnsupportedCartesianAtomSites,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CifFinding {
    pub severity: CifFindingSeverity,
    pub code: CifFindingCode,
    pub message: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct CifValidationPolicy {
    pub require_explicit_symmetry_declarations: bool,
    pub allow_missing_occupancy: bool,
    pub allow_out_of_range_occupancy: bool,
    pub allow_overfilled_disorder: bool,
    pub allow_overlapping_expanded_sites: bool,
}

impl CifValidationPolicy {
    pub fn strict() -> Self {
        Self {
            require_explicit_symmetry_declarations: true,
            allow_missing_occupancy: false,
            allow_out_of_range_occupancy: false,
            allow_overfilled_disorder: false,
            allow_overlapping_expanded_sites: false,
        }
    }

    pub fn scientific_default() -> Self {
        Self {
            require_explicit_symmetry_declarations: false,
            allow_missing_occupancy: true,
            allow_out_of_range_occupancy: false,
            allow_overfilled_disorder: false,
            allow_overlapping_expanded_sites: false,
        }
    }
}

impl Default for CifValidationPolicy {
    fn default() -> Self {
        Self::scientific_default()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CifRadiusMode {
    Covalent,
    Ionic,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CifSpeciesRadius {
    pub species: String,
    pub covalent_radius: f64,
    pub ionic_radius: f64,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct CifGeometryPolicy {
    pub duplicate_tolerance_cartesian: f64,
    pub short_contact_scale: f64,
    pub radius_mode: CifRadiusMode,
    pub fallback_radius: Option<f64>,
    pub allow_unknown_species_radii: bool,
    pub allow_duplicate_sites: bool,
    pub allow_short_contacts: bool,
}

impl CifGeometryPolicy {
    pub fn strict() -> Self {
        Self {
            duplicate_tolerance_cartesian: 1.0e-5,
            short_contact_scale: 0.6,
            radius_mode: CifRadiusMode::Covalent,
            fallback_radius: None,
            allow_unknown_species_radii: false,
            allow_duplicate_sites: false,
            allow_short_contacts: false,
        }
    }

    pub fn scientific_default() -> Self {
        Self {
            duplicate_tolerance_cartesian: 1.0e-5,
            short_contact_scale: 0.6,
            radius_mode: CifRadiusMode::Covalent,
            fallback_radius: None,
            allow_unknown_species_radii: true,
            allow_duplicate_sites: false,
            allow_short_contacts: false,
        }
    }
}

impl Default for CifGeometryPolicy {
    fn default() -> Self {
        Self::scientific_default()
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CifScientificPolicy {
    pub semantics: CifValidationPolicy,
    pub geometry: Option<CifGeometryPolicy>,
}

impl CifScientificPolicy {
    pub fn strict() -> Self {
        Self {
            semantics: CifValidationPolicy::strict(),
            geometry: Some(CifGeometryPolicy::strict()),
        }
    }

    pub fn scientific_default() -> Self {
        Self {
            semantics: CifValidationPolicy::scientific_default(),
            geometry: Some(CifGeometryPolicy::scientific_default()),
        }
    }
}

impl Default for CifScientificPolicy {
    fn default() -> Self {
        Self::scientific_default()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct CifCell {
    pub lengths: [f64; 3],
    pub angles_deg: [f64; 3],
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CifAtomSite {
    pub label: Option<String>,
    pub species: String,
    pub fractional: [f64; 3],
    pub occupancy: Option<f64>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CifExpandedSite {
    pub species: String,
    pub fractional: [f64; 3],
    pub source_label: Option<String>,
    pub operation_index: usize,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CifDataBlock {
    pub name: String,
    pub dialect: CifDialect,
    pub cell: CifCell,
    pub atom_sites: Vec<CifAtomSite>,
    pub symmetry_operations: Vec<CifSymmetryOperation>,
    pub expanded_sites: Vec<CifExpandedSite>,
    pub findings: Vec<CifFinding>,
}

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum CifError {
    #[error("empty cif input")]
    EmptyInput,
    #[error("missing cif data block header")]
    MissingDataBlock,
    #[error("missing required cell parameter `{parameter}`")]
    MissingCellParameter { parameter: &'static str },
    #[error("missing required atom-site fractional columns")]
    MissingFractionalAtomSiteColumns,
    #[error("no atom sites found in cif block")]
    MissingAtomSites,
    #[error("failed to parse cif float `{raw}`")]
    InvalidFloat { raw: String },
    #[error("failed to parse cif fraction `{raw}`")]
    InvalidFraction { raw: String },
    #[error(
        "failed to parse cif symmetry operation `{raw}`: expected three comma-separated components"
    )]
    InvalidSymmetryOperation { raw: String },
    #[error("failed to parse cif symmetry term `{raw}`")]
    InvalidSymmetryTerm { raw: String },
    #[error("{message}")]
    Io { message: String },
    #[cfg(feature = "bridge-patina-types")]
    #[error("invalid candidate for cif bridge: {message}")]
    InvalidCandidate { message: String },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CifSymmetryOperation {
    pub coefficients: [[f64; 3]; 3],
    pub offsets: [f64; 3],
}

impl CifSymmetryOperation {
    pub fn identity() -> Self {
        Self {
            coefficients: [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]],
            offsets: [0.0, 0.0, 0.0],
        }
    }

    pub fn apply(&self, fractional: [f64; 3]) -> [f64; 3] {
        normalize_fractional_coordinate([
            dot_fractional_row(self.coefficients[0], fractional) + self.offsets[0],
            dot_fractional_row(self.coefficients[1], fractional) + self.offsets[1],
            dot_fractional_row(self.coefficients[2], fractional) + self.offsets[2],
        ])
    }
}

pub fn read_cif_path(path: &Path) -> Result<CifDataBlock, CifError> {
    let text = fs::read_to_string(path).map_err(|err| CifError::Io {
        message: format!("failed to read `{}`: {err}", path.display()),
    })?;
    parse_cif_str(&text)
}

pub fn parse_cif_str(text: &str) -> Result<CifDataBlock, CifError> {
    let lines = text.lines().collect::<Vec<_>>();
    if lines.is_empty() {
        return Err(CifError::EmptyInput);
    }

    let mut name = None::<String>;
    let mut dialect = CifDialect::Core;
    let mut cell_length_a = None::<f64>;
    let mut cell_length_b = None::<f64>;
    let mut cell_length_c = None::<f64>;
    let mut cell_angle_alpha = None::<f64>;
    let mut cell_angle_beta = None::<f64>;
    let mut cell_angle_gamma = None::<f64>;
    let mut atom_sites = Vec::new();
    let mut symmetry_operations = Vec::new();
    let mut findings = Vec::new();
    let mut idx = 0usize;

    while idx < lines.len() {
        let line = lines[idx].trim();
        if line.is_empty() || line.starts_with('#') {
            idx += 1;
            continue;
        }

        if let Some(block_name) = line.strip_prefix("data_") {
            name = Some(block_name.trim().to_string());
            idx += 1;
            continue;
        }

        if let Some(value) = parse_tagged_value(line, CELL_LENGTH_A_HEADERS) {
            cell_length_a = Some(parse_cif_float(value)?);
            dialect = dialect_for_header(CELL_LENGTH_A_HEADERS[0], line);
            idx += 1;
            continue;
        }
        if let Some(value) = parse_tagged_value(line, CELL_LENGTH_B_HEADERS) {
            cell_length_b = Some(parse_cif_float(value)?);
            dialect = dialect_for_header(CELL_LENGTH_B_HEADERS[0], line);
            idx += 1;
            continue;
        }
        if let Some(value) = parse_tagged_value(line, CELL_LENGTH_C_HEADERS) {
            cell_length_c = Some(parse_cif_float(value)?);
            dialect = dialect_for_header(CELL_LENGTH_C_HEADERS[0], line);
            idx += 1;
            continue;
        }
        if let Some(value) = parse_tagged_value(line, CELL_ANGLE_ALPHA_HEADERS) {
            cell_angle_alpha = Some(parse_cif_float(value)?);
            dialect = dialect_for_header(CELL_ANGLE_ALPHA_HEADERS[0], line);
            idx += 1;
            continue;
        }
        if let Some(value) = parse_tagged_value(line, CELL_ANGLE_BETA_HEADERS) {
            cell_angle_beta = Some(parse_cif_float(value)?);
            dialect = dialect_for_header(CELL_ANGLE_BETA_HEADERS[0], line);
            idx += 1;
            continue;
        }
        if let Some(value) = parse_tagged_value(line, CELL_ANGLE_GAMMA_HEADERS) {
            cell_angle_gamma = Some(parse_cif_float(value)?);
            dialect = dialect_for_header(CELL_ANGLE_GAMMA_HEADERS[0], line);
            idx += 1;
            continue;
        }

        if let Some(operation_raw) = parse_cif_symmetry_inline_line(line) {
            symmetry_operations.push(parse_cif_symmetry_operation(&operation_raw)?);
            idx += 1;
            continue;
        }

        if line == "loop_" {
            idx += 1;
            let mut headers = Vec::new();
            while idx < lines.len() {
                let header = lines[idx].trim();
                if !header.starts_with('_') {
                    break;
                }
                if header.contains('.') {
                    dialect = CifDialect::Mmcif;
                }
                headers.push(header.to_string());
                idx += 1;
            }

            if let Some(symop_idx) = headers
                .iter()
                .position(|header| is_cif_symmetry_header(header))
            {
                while idx < lines.len() {
                    let row = lines[idx].trim();
                    if is_loop_terminator(row) {
                        break;
                    }
                    let parts = tokenize_cif_row(row);
                    if let Some(operation_raw) = parts.get(symop_idx) {
                        symmetry_operations.push(parse_cif_symmetry_operation(operation_raw)?);
                    }
                    idx += 1;
                }
                continue;
            }

            let label_idx = header_index(&headers, ATOM_SITE_LABEL_HEADERS);
            let species_idx = header_index(&headers, ATOM_SITE_TYPE_SYMBOL_HEADERS).or(label_idx);
            let fract_x_idx = header_index(&headers, ATOM_SITE_FRACT_X_HEADERS);
            let fract_y_idx = header_index(&headers, ATOM_SITE_FRACT_Y_HEADERS);
            let fract_z_idx = header_index(&headers, ATOM_SITE_FRACT_Z_HEADERS);
            let occupancy_idx = header_index(&headers, ATOM_SITE_OCCUPANCY_HEADERS);
            let has_cartesian_sites = header_index(&headers, ATOM_SITE_CARTN_X_HEADERS).is_some()
                || header_index(&headers, ATOM_SITE_CARTN_Y_HEADERS).is_some()
                || header_index(&headers, ATOM_SITE_CARTN_Z_HEADERS).is_some();

            if has_cartesian_sites
                && fract_x_idx.is_none()
                && fract_y_idx.is_none()
                && fract_z_idx.is_none()
            {
                findings.push(CifFinding {
                    severity: CifFindingSeverity::Warning,
                    code: CifFindingCode::UnsupportedCartesianAtomSites,
                    message: "atom sites are provided only in Cartesian CIF columns; the current kernel requires fractional atom-site coordinates".to_string(),
                });
            }

            if let (Some(species_idx), Some(fract_x_idx), Some(fract_y_idx), Some(fract_z_idx)) =
                (species_idx, fract_x_idx, fract_y_idx, fract_z_idx)
            {
                while idx < lines.len() {
                    let row = lines[idx].trim();
                    if is_loop_terminator(row) {
                        break;
                    }
                    let parts = tokenize_cif_row(row);
                    if parts.len() <= fract_z_idx || parts.len() <= species_idx {
                        idx += 1;
                        continue;
                    }
                    let occupancy = occupancy_idx
                        .and_then(|column| parts.get(column))
                        .map(|value| parse_cif_float(value))
                        .transpose()?;
                    let atom_site = CifAtomSite {
                        label: label_idx.and_then(|column| parts.get(column).cloned()),
                        species: normalize_cif_species(&parts[species_idx]),
                        fractional: [
                            parse_cif_float(&parts[fract_x_idx])?,
                            parse_cif_float(&parts[fract_y_idx])?,
                            parse_cif_float(&parts[fract_z_idx])?,
                        ],
                        occupancy,
                    };
                    atom_sites.push(atom_site);
                    idx += 1;
                }
                continue;
            }
        }

        idx += 1;
    }

    let name = name.ok_or(CifError::MissingDataBlock)?;
    let cell = CifCell {
        lengths: [
            cell_length_a.ok_or(CifError::MissingCellParameter {
                parameter: CELL_LENGTH_A_HEADERS[0],
            })?,
            cell_length_b.ok_or(CifError::MissingCellParameter {
                parameter: CELL_LENGTH_B_HEADERS[0],
            })?,
            cell_length_c.ok_or(CifError::MissingCellParameter {
                parameter: CELL_LENGTH_C_HEADERS[0],
            })?,
        ],
        angles_deg: [
            cell_angle_alpha.ok_or(CifError::MissingCellParameter {
                parameter: CELL_ANGLE_ALPHA_HEADERS[0],
            })?,
            cell_angle_beta.ok_or(CifError::MissingCellParameter {
                parameter: CELL_ANGLE_BETA_HEADERS[0],
            })?,
            cell_angle_gamma.ok_or(CifError::MissingCellParameter {
                parameter: CELL_ANGLE_GAMMA_HEADERS[0],
            })?,
        ],
    };

    if atom_sites.is_empty() {
        if findings
            .iter()
            .any(|finding| finding.code == CifFindingCode::UnsupportedCartesianAtomSites)
        {
            return Err(CifError::MissingFractionalAtomSiteColumns);
        }
        return Err(CifError::MissingAtomSites);
    }

    collect_atom_site_findings(&atom_sites, &mut findings);

    if symmetry_operations.is_empty() {
        findings.push(CifFinding {
            severity: CifFindingSeverity::Warning,
            code: CifFindingCode::MissingSymmetryOperations,
            message:
                "no CIF symmetry operations were declared; falling back to the identity operation"
                    .to_string(),
        });
        symmetry_operations.push(CifSymmetryOperation::identity());
    }

    let expanded_sites = expand_cif_atom_sites(&atom_sites, &symmetry_operations, &mut findings);

    Ok(CifDataBlock {
        name,
        dialect,
        cell,
        atom_sites,
        symmetry_operations,
        expanded_sites,
        findings,
    })
}

#[cfg(feature = "bridge-patina-types")]
pub fn candidate_from_cif_path(path: &Path) -> Result<Candidate, CifError> {
    let block = read_cif_path(path)?;
    let label = path
        .file_stem()
        .and_then(|stem| stem.to_str())
        .unwrap_or("candidate");
    candidate_from_cif_block(&block, label)
}

#[cfg(feature = "bridge-patina-types")]
pub fn candidate_from_cif_path_with_policy(
    path: &Path,
    policy: &CifValidationPolicy,
) -> Result<ConversionOutcome<Candidate>, CifBridgeError> {
    let block = read_cif_path(path).map_err(CifBridgeError::Parse)?;
    let label = path
        .file_stem()
        .and_then(|stem| stem.to_str())
        .unwrap_or("candidate");
    candidate_from_cif_block_with_policy(&block, label, policy)
}

#[cfg(feature = "bridge-patina-types")]
pub fn candidate_from_cif_block(
    block: &CifDataBlock,
    label: impl Into<String>,
) -> Result<Candidate, CifError> {
    let lattice = lattice_vectors_from_cell(block.cell);
    let candidate = Candidate::fully_periodic(
        label,
        block
            .expanded_sites
            .iter()
            .map(|site| site.species.clone())
            .collect(),
        block
            .expanded_sites
            .iter()
            .map(|site| site.fractional)
            .collect(),
        lattice,
    );
    candidate
        .validate()
        .map_err(|err| CifError::InvalidCandidate {
            message: format!("{err:?}"),
        })?;
    Ok(candidate)
}

pub fn validate_cif_block(
    block: &CifDataBlock,
    policy: &CifValidationPolicy,
) -> Result<ConversionOutcome<()>, ConversionError> {
    let mut outcome = ConversionOutcome::new(ConversionKind::StructureValidation, ());
    for finding in &block.findings {
        let warning = cif_finding_to_warning(finding);
        match finding.code {
            CifFindingCode::MissingSymmetryOperations => {
                if policy.require_explicit_symmetry_declarations {
                    return Err(ConversionError::InferenceRequired {
                        message: finding.message.clone(),
                    });
                }
                outcome.warnings.push(warning);
            }
            CifFindingCode::MissingOccupancy => {
                if !policy.allow_missing_occupancy {
                    return Err(ConversionError::Unsupported {
                        message: finding.message.clone(),
                    });
                }
                outcome.warnings.push(warning);
            }
            CifFindingCode::OccupancyOutOfRange => {
                if !policy.allow_out_of_range_occupancy {
                    return Err(ConversionError::Unsupported {
                        message: finding.message.clone(),
                    });
                }
                outcome.warnings.push(warning);
            }
            CifFindingCode::OccupancySumExceedsOne => {
                if !policy.allow_overfilled_disorder {
                    return Err(ConversionError::Unsupported {
                        message: finding.message.clone(),
                    });
                }
                outcome.warnings.push(warning);
            }
            CifFindingCode::OverlappingExpandedSites => {
                if !policy.allow_overlapping_expanded_sites {
                    return Err(ConversionError::Unsupported {
                        message: finding.message.clone(),
                    });
                }
                outcome.warnings.push(warning);
            }
            CifFindingCode::UnsupportedCartesianAtomSites => {
                return Err(ConversionError::Unsupported {
                    message: finding.message.clone(),
                });
            }
        }
    }
    Ok(outcome)
}

pub fn validate_cif_geometry(
    block: &CifDataBlock,
    policy: &CifGeometryPolicy,
    species_radii: &[CifSpeciesRadius],
) -> Result<ConversionOutcome<()>, ConversionError> {
    let lattice = lattice_vectors_from_cell(block.cell);
    let mut outcome = ConversionOutcome::new(ConversionKind::StructureValidation, ());
    for left_index in 0..block.expanded_sites.len() {
        for right_index in (left_index + 1)..block.expanded_sites.len() {
            let left = &block.expanded_sites[left_index];
            let right = &block.expanded_sites[right_index];
            let left_cart = geometry_fractional_to_cartesian(lattice, left.fractional);
            let right_cart = geometry_fractional_to_cartesian(lattice, right.fractional);
            let distance_sq = geometry_minimum_image_cartesian_distance_sq_with_axes(
                left_cart,
                right_cart,
                Some(lattice),
                [true, true, true],
            );
            let distance = distance_sq.sqrt();

            if distance <= policy.duplicate_tolerance_cartesian {
                let message = format!(
                    "expanded CIF sites `{}` and `{}` collapse to the same Cartesian position within {:.3e} A",
                    left.source_label.as_deref().unwrap_or(left.species.as_str()),
                    right.source_label.as_deref().unwrap_or(right.species.as_str()),
                    policy.duplicate_tolerance_cartesian,
                );
                if !policy.allow_duplicate_sites {
                    return Err(ConversionError::Unsupported { message });
                }
                outcome.warnings.push(ConversionWarning::new(message));
                continue;
            }

            let left_radius = species_radius_for_species(&left.species, species_radii, policy);
            let right_radius = species_radius_for_species(&right.species, species_radii, policy);
            match (left_radius, right_radius) {
                (Some(left_radius), Some(right_radius)) => {
                    let threshold = (left_radius + right_radius) * policy.short_contact_scale;
                    if distance + 1.0e-12 < threshold {
                        let message = format!(
                            "expanded CIF sites `{}` and `{}` form a short contact: {:.6} A < {:.6} A threshold",
                            left.source_label.as_deref().unwrap_or(left.species.as_str()),
                            right.source_label.as_deref().unwrap_or(right.species.as_str()),
                            distance,
                            threshold,
                        );
                        if !policy.allow_short_contacts {
                            return Err(ConversionError::Unsupported { message });
                        }
                        outcome.warnings.push(ConversionWarning::new(message));
                    }
                }
                (None, _) | (_, None) if !policy.allow_unknown_species_radii => {
                    let missing_species = if left_radius.is_none() {
                        left.species.as_str()
                    } else {
                        right.species.as_str()
                    };
                    return Err(ConversionError::Unsupported {
                        message: format!(
                            "missing {:?} radius for species `{missing_species}` during CIF geometric validation",
                            policy.radius_mode
                        ),
                    });
                }
                (None, _) | (_, None) => {
                    let missing_species = if left_radius.is_none() {
                        left.species.as_str()
                    } else {
                        right.species.as_str()
                    };
                    outcome.warnings.push(ConversionWarning::new(format!(
                        "missing {:?} radius for species `{missing_species}` during CIF geometric validation; short-contact screening skipped for at least one pair",
                        policy.radius_mode
                    )));
                }
            }
        }
    }
    Ok(outcome)
}

pub fn validate_cif_for_scientific_use(
    block: &CifDataBlock,
    policy: &CifScientificPolicy,
    species_radii: &[CifSpeciesRadius],
) -> Result<ConversionOutcome<()>, ConversionError> {
    let mut outcome = validate_cif_block(block, &policy.semantics)?;
    if let Some(geometry_policy) = &policy.geometry {
        let geometry = validate_cif_geometry(block, geometry_policy, species_radii)?;
        outcome.warnings.extend(geometry.warnings);
    }
    Ok(outcome)
}

#[cfg(feature = "bridge-patina-types")]
pub fn candidate_from_cif_block_with_policy(
    block: &CifDataBlock,
    label: impl Into<String>,
    policy: &CifValidationPolicy,
) -> Result<ConversionOutcome<Candidate>, CifBridgeError> {
    let validation = validate_cif_block(block, policy).map_err(CifBridgeError::Conversion)?;
    let candidate = candidate_from_cif_block(block, label).map_err(CifBridgeError::Parse)?;
    Ok(ConversionOutcome {
        kind: ConversionKind::RepresentationCodec,
        value: candidate,
        warnings: validation.warnings,
    })
}

#[cfg(feature = "bridge-patina-types")]
pub fn candidate_from_cif_block_for_scientific_use(
    block: &CifDataBlock,
    label: impl Into<String>,
    policy: &CifScientificPolicy,
    species_radii: &[CifSpeciesRadius],
) -> Result<ConversionOutcome<Candidate>, CifBridgeError> {
    let validation = validate_cif_for_scientific_use(block, policy, species_radii)
        .map_err(CifBridgeError::Conversion)?;
    let candidate = candidate_from_cif_block(block, label).map_err(CifBridgeError::Parse)?;
    Ok(ConversionOutcome {
        kind: ConversionKind::RepresentationCodec,
        value: candidate,
        warnings: validation.warnings,
    })
}

pub fn parse_cif_symmetry_operation(raw: &str) -> Result<CifSymmetryOperation, CifError> {
    let cleaned = trim_cif_quotes(raw).replace(' ', "");
    let components = cleaned.split(',').collect::<Vec<_>>();
    if components.len() != 3 {
        return Err(CifError::InvalidSymmetryOperation {
            raw: raw.to_string(),
        });
    }
    let mut coefficients = [[0.0; 3]; 3];
    let mut offsets = [0.0; 3];
    for (index, component) in components.iter().enumerate() {
        let (row, offset) = parse_cif_symmetry_component(component)?;
        coefficients[index] = row;
        offsets[index] = offset;
    }
    Ok(CifSymmetryOperation {
        coefficients,
        offsets,
    })
}

const CELL_LENGTH_A_HEADERS: &[&str] = &["_cell_length_a", "_cell.length_a"];
const CELL_LENGTH_B_HEADERS: &[&str] = &["_cell_length_b", "_cell.length_b"];
const CELL_LENGTH_C_HEADERS: &[&str] = &["_cell_length_c", "_cell.length_c"];
const CELL_ANGLE_ALPHA_HEADERS: &[&str] = &["_cell_angle_alpha", "_cell.angle_alpha"];
const CELL_ANGLE_BETA_HEADERS: &[&str] = &["_cell_angle_beta", "_cell.angle_beta"];
const CELL_ANGLE_GAMMA_HEADERS: &[&str] = &["_cell_angle_gamma", "_cell.angle_gamma"];
const ATOM_SITE_LABEL_HEADERS: &[&str] = &["_atom_site_label", "_atom_site.label"];
const ATOM_SITE_TYPE_SYMBOL_HEADERS: &[&str] =
    &["_atom_site_type_symbol", "_atom_site.type_symbol"];
const ATOM_SITE_FRACT_X_HEADERS: &[&str] = &["_atom_site_fract_x", "_atom_site.fract_x"];
const ATOM_SITE_FRACT_Y_HEADERS: &[&str] = &["_atom_site_fract_y", "_atom_site.fract_y"];
const ATOM_SITE_FRACT_Z_HEADERS: &[&str] = &["_atom_site_fract_z", "_atom_site.fract_z"];
const ATOM_SITE_CARTN_X_HEADERS: &[&str] = &["_atom_site_Cartn_x", "_atom_site.Cartn_x"];
const ATOM_SITE_CARTN_Y_HEADERS: &[&str] = &["_atom_site_Cartn_y", "_atom_site.Cartn_y"];
const ATOM_SITE_CARTN_Z_HEADERS: &[&str] = &["_atom_site_Cartn_z", "_atom_site.Cartn_z"];
const ATOM_SITE_OCCUPANCY_HEADERS: &[&str] = &["_atom_site_occupancy", "_atom_site.occupancy"];
const CIF_SYMMETRY_HEADERS: &[&str] = &[
    "_space_group_symop_operation_xyz",
    "_space_group_symop.operation_xyz",
    "_symmetry_equiv_pos_as_xyz",
    "_symmetry_equiv.pos_as_xyz",
];

#[cfg(feature = "bridge-patina-types")]
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum CifBridgeError {
    #[error("{0}")]
    Parse(CifError),
    #[error("{0:?}")]
    Conversion(ConversionError),
}

fn cif_finding_to_warning(finding: &CifFinding) -> ConversionWarning {
    let mut warning = ConversionWarning::new(finding.message.clone());
    if matches!(finding.code, CifFindingCode::MissingSymmetryOperations) {
        warning.inference = Some(InferenceKind::Symmetry);
    }
    warning
}

fn species_radius_for_species(
    species: &str,
    species_radii: &[CifSpeciesRadius],
    policy: &CifGeometryPolicy,
) -> Option<f64> {
    if let Some(record) = species_radii
        .iter()
        .find(|record| record.species == species)
    {
        return Some(match policy.radius_mode {
            CifRadiusMode::Covalent => record.covalent_radius,
            CifRadiusMode::Ionic => record.ionic_radius,
        });
    }
    policy.fallback_radius
}

fn collect_atom_site_findings(atom_sites: &[CifAtomSite], findings: &mut Vec<CifFinding>) {
    for site in atom_sites {
        match site.occupancy {
            None => findings.push(CifFinding {
                severity: CifFindingSeverity::Warning,
                code: CifFindingCode::MissingOccupancy,
                message: format!(
                    "atom site `{}` is missing occupancy; downstream validation should treat this as an assumed full site unless a dialect-specific policy overrides it",
                    site.label.as_deref().unwrap_or(site.species.as_str())
                ),
            }),
            Some(occupancy) if !(0.0..=1.0).contains(&occupancy) => findings.push(CifFinding {
                severity: CifFindingSeverity::Warning,
                code: CifFindingCode::OccupancyOutOfRange,
                message: format!(
                    "atom site `{}` declared occupancy {occupancy}, which falls outside the normal [0, 1] interval",
                    site.label.as_deref().unwrap_or(site.species.as_str())
                ),
            }),
            Some(_) => {}
        }
    }

    for (index, site) in atom_sites.iter().enumerate() {
        let Some(label) = site.label.as_deref() else {
            continue;
        };
        let occupancy_sum = atom_sites
            .iter()
            .skip(index)
            .filter(|candidate| candidate.label.as_deref() == Some(label))
            .filter(|candidate| fractional_close(candidate.fractional, site.fractional, 1.0e-6))
            .map(|candidate| candidate.occupancy.unwrap_or(1.0))
            .sum::<f64>();
        if occupancy_sum > 1.0 + 1.0e-6 {
            findings.push(CifFinding {
                severity: CifFindingSeverity::Warning,
                code: CifFindingCode::OccupancySumExceedsOne,
                message: format!(
                    "disordered occupancy for atom site `{label}` sums to {occupancy_sum:.6}, which suggests an invalid or overfilled site model"
                ),
            });
        }
    }
}

fn expand_cif_atom_sites(
    atom_sites: &[CifAtomSite],
    operations: &[CifSymmetryOperation],
    findings: &mut Vec<CifFinding>,
) -> Vec<CifExpandedSite> {
    let mut expanded = Vec::<CifExpandedSite>::new();
    for atom_site in atom_sites {
        for (operation_index, operation) in operations.iter().enumerate() {
            let transformed = operation.apply(atom_site.fractional);
            if expanded.iter().any(|existing| {
                existing.species == atom_site.species
                    && fractional_close(existing.fractional, transformed, 1.0e-6)
            }) {
                continue;
            }
            if let Some(existing) = expanded.iter().find(|existing| {
                fractional_close(existing.fractional, transformed, 1.0e-6)
                    && existing.species != atom_site.species
            }) {
                findings.push(CifFinding {
                    severity: CifFindingSeverity::Warning,
                    code: CifFindingCode::OverlappingExpandedSites,
                    message: format!(
                        "expanded symmetry sites overlap at [{:.6}, {:.6}, {:.6}] for species `{}` and `{}`",
                        transformed[0], transformed[1], transformed[2], existing.species, atom_site.species
                    ),
                });
            }
            expanded.push(CifExpandedSite {
                species: atom_site.species.clone(),
                fractional: transformed,
                source_label: atom_site.label.clone(),
                operation_index,
            });
        }
    }
    expanded
}

fn header_index(headers: &[String], aliases: &[&str]) -> Option<usize> {
    headers
        .iter()
        .position(|header| matches_alias(header, aliases))
}

fn parse_tagged_value<'a>(line: &'a str, aliases: &[&str]) -> Option<&'a str> {
    for alias in aliases {
        if let Some(rest) = line.strip_prefix(alias) {
            let value = rest.trim();
            if !value.is_empty() {
                return Some(value);
            }
        }
    }
    None
}

fn dialect_for_header(default_alias: &str, header: &str) -> CifDialect {
    if header.contains('.') && !default_alias.contains('.') {
        CifDialect::Mmcif
    } else {
        CifDialect::Core
    }
}

fn matches_alias(header: &str, aliases: &[&str]) -> bool {
    aliases
        .iter()
        .any(|alias| header.eq_ignore_ascii_case(alias))
}

fn is_loop_terminator(line: &str) -> bool {
    line.is_empty()
        || line.starts_with('#')
        || line.starts_with("loop_")
        || line.starts_with('_')
        || line.starts_with("data_")
}

fn parse_cif_symmetry_inline_line(line: &str) -> Option<String> {
    for key in CIF_SYMMETRY_HEADERS {
        if let Some(rest) = line.strip_prefix(key) {
            let value = rest.trim();
            if !value.is_empty() {
                return Some(trim_cif_quotes(value).to_string());
            }
        }
    }
    None
}

fn is_cif_symmetry_header(header: &str) -> bool {
    matches_alias(header, CIF_SYMMETRY_HEADERS)
}

fn parse_cif_symmetry_component(raw: &str) -> Result<([f64; 3], f64), CifError> {
    let mut coefficients = [0.0; 3];
    let mut offset = 0.0;
    let chars = raw.chars().collect::<Vec<_>>();
    let mut idx = 0usize;

    while idx < chars.len() {
        let mut sign = 1.0;
        match chars[idx] {
            '+' => idx += 1,
            '-' => {
                sign = -1.0;
                idx += 1;
            }
            _ => {}
        }

        let start = idx;
        while idx < chars.len() && chars[idx] != '+' && chars[idx] != '-' {
            idx += 1;
        }
        let term = &raw[start..idx];
        if term.is_empty() {
            continue;
        }

        if let Some(axis) = term
            .chars()
            .find(|ch| matches!(ch, 'x' | 'y' | 'z' | 'X' | 'Y' | 'Z'))
        {
            let axis_idx = match axis.to_ascii_lowercase() {
                'x' => 0,
                'y' => 1,
                'z' => 2,
                _ => unreachable!(),
            };
            let prefix = term
                .chars()
                .take_while(|ch| !matches!(ch, 'x' | 'y' | 'z' | 'X' | 'Y' | 'Z'))
                .collect::<String>();
            let magnitude = if prefix.is_empty() {
                1.0
            } else {
                parse_cif_fraction(&prefix)?
            };
            coefficients[axis_idx] += sign * magnitude;
        } else {
            offset += sign * parse_cif_fraction(term)?;
        }
    }

    Ok((coefficients, offset))
}

fn parse_cif_fraction(raw: &str) -> Result<f64, CifError> {
    if let Some((numerator, denominator)) = raw.split_once('/') {
        let numerator = parse_cif_float(numerator)?;
        let denominator = parse_cif_float(denominator)?;
        if denominator.abs() < 1.0e-12 {
            return Err(CifError::InvalidFraction {
                raw: raw.to_string(),
            });
        }
        Ok(numerator / denominator)
    } else {
        parse_cif_float(raw)
    }
}

fn parse_cif_float(raw: &str) -> Result<f64, CifError> {
    let cleaned = raw.trim().trim_matches('\'').trim_matches('"');
    let cleaned = cleaned
        .split('(')
        .next()
        .unwrap_or(cleaned)
        .trim_end_matches([',', ';']);
    cleaned.parse::<f64>().map_err(|_| CifError::InvalidFloat {
        raw: raw.to_string(),
    })
}

fn tokenize_cif_row(row: &str) -> Vec<String> {
    let mut tokens = Vec::new();
    let mut current = String::new();
    let mut quote = None::<char>;
    for ch in row.chars() {
        match quote {
            Some(active_quote) if ch == active_quote => {
                quote = None;
            }
            Some(_) => current.push(ch),
            None if ch == '\'' || ch == '"' => {
                quote = Some(ch);
            }
            None if ch.is_whitespace() => {
                if !current.is_empty() {
                    tokens.push(current.clone());
                    current.clear();
                }
            }
            None => current.push(ch),
        }
    }
    if !current.is_empty() {
        tokens.push(current);
    }
    tokens
}

fn trim_cif_quotes(raw: &str) -> &str {
    raw.trim().trim_matches('\'').trim_matches('"')
}

fn normalize_cif_species(raw: &str) -> String {
    let cleaned = trim_cif_quotes(raw);
    let letters = cleaned
        .chars()
        .take_while(|ch| ch.is_ascii_alphabetic())
        .collect::<String>();
    if letters.is_empty() {
        cleaned.to_string()
    } else {
        letters
    }
}

fn lattice_vectors_from_cell(cell: CifCell) -> [[f64; 3]; 3] {
    let [a, b, c] = cell.lengths;
    let [alpha_deg, beta_deg, gamma_deg] = cell.angles_deg;
    let alpha = alpha_deg.to_radians();
    let beta = beta_deg.to_radians();
    let gamma = gamma_deg.to_radians();
    let cos_alpha = alpha.cos();
    let cos_beta = beta.cos();
    let cos_gamma = gamma.cos();
    let sin_gamma = gamma.sin();
    let volume_factor = (1.0 - cos_alpha * cos_alpha - cos_beta * cos_beta - cos_gamma * cos_gamma
        + 2.0 * cos_alpha * cos_beta * cos_gamma)
        .max(0.0)
        .sqrt();

    [
        [a, 0.0, 0.0],
        [b * cos_gamma, b * sin_gamma, 0.0],
        [
            c * cos_beta,
            c * (cos_alpha - cos_beta * cos_gamma) / sin_gamma,
            c * volume_factor / sin_gamma,
        ],
    ]
}

fn normalize_fractional_coordinate(mut fractional: [f64; 3]) -> [f64; 3] {
    for value in &mut fractional {
        *value = value.rem_euclid(1.0);
        if (*value - 1.0).abs() <= 1.0e-12 {
            *value = 0.0;
        }
    }
    fractional
}

fn fractional_close(left: [f64; 3], right: [f64; 3], tolerance: f64) -> bool {
    (left[0] - right[0]).abs() <= tolerance
        && (left[1] - right[1]).abs() <= tolerance
        && (left[2] - right[2]).abs() <= tolerance
}

fn dot_fractional_row(coefficients: [f64; 3], fractional: [f64; 3]) -> f64 {
    coefficients[0] * fractional[0]
        + coefficients[1] * fractional[1]
        + coefficients[2] * fractional[2]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn symmetry_operation_parser_handles_fractional_offsets() {
        let operation = parse_cif_symmetry_operation("-x+1/2,y+1/4,z").unwrap();
        let transformed = operation.apply([0.1, 0.2, 0.3]);
        assert!((transformed[0] - 0.4).abs() < 1.0e-12);
        assert!((transformed[1] - 0.45).abs() < 1.0e-12);
        assert!((transformed[2] - 0.3).abs() < 1.0e-12);
    }

    #[test]
    fn cif_parser_expands_asymmetric_unit_with_symmetry_operations() {
        let block = parse_cif_str(
            "data_test
_cell_length_a 4.0
_cell_length_b 4.0
_cell_length_c 4.0
_cell_angle_alpha 90
_cell_angle_beta 90
_cell_angle_gamma 90
loop_
_space_group_symop_operation_xyz
'x, y, z'
'-x, -y, -z'
loop_
_atom_site_type_symbol
_atom_site_fract_x
_atom_site_fract_y
_atom_site_fract_z
Mg 0.1 0.2 0.3
",
        )
        .unwrap();

        assert_eq!(block.expanded_sites.len(), 2);
        assert_eq!(block.expanded_sites[0].species, "Mg");
        assert!((block.expanded_sites[0].fractional[0] - 0.1).abs() < 1.0e-12);
        assert!((block.expanded_sites[1].fractional[0] - 0.9).abs() < 1.0e-12);
    }

    #[test]
    fn cif_parser_reads_atom_sites_and_reports_missing_occupancy() {
        let block = parse_cif_str(
            "data_test
_cell_length_a 4.0
_cell_length_b 5.0
_cell_length_c 6.0
_cell_angle_alpha 90
_cell_angle_beta 90
_cell_angle_gamma 120
loop_
_atom_site_label
_atom_site_fract_x
_atom_site_fract_y
_atom_site_fract_z
Mg1 0.0 0.0 0.0
O1 0.5 0.5 0.5
",
        )
        .unwrap();

        assert_eq!(block.atom_sites.len(), 2);
        assert_eq!(block.atom_sites[0].species, "Mg");
        assert!(block
            .findings
            .iter()
            .any(|finding| { finding.code == CifFindingCode::MissingOccupancy }));
        assert!(block
            .findings
            .iter()
            .any(|finding| { finding.code == CifFindingCode::MissingSymmetryOperations }));
    }

    #[test]
    fn cif_parser_reports_overfilled_disorder_sites() {
        let block = parse_cif_str(
            "data_test
_cell_length_a 4.0
_cell_length_b 4.0
_cell_length_c 4.0
_cell_angle_alpha 90
_cell_angle_beta 90
_cell_angle_gamma 90
loop_
_atom_site_label
_atom_site_type_symbol
_atom_site_fract_x
_atom_site_fract_y
_atom_site_fract_z
_atom_site_occupancy
M1 Mg 0.0 0.0 0.0 0.8
M1 Zn 0.0 0.0 0.0 0.7
",
        )
        .unwrap();

        assert!(block
            .findings
            .iter()
            .any(|finding| { finding.code == CifFindingCode::OccupancySumExceedsOne }));
        assert!(block
            .findings
            .iter()
            .any(|finding| { finding.code == CifFindingCode::OverlappingExpandedSites }));
    }

    #[test]
    #[cfg(feature = "bridge-patina-types")]
    fn cif_candidate_bridge_preserves_fractional_periodic_structure() {
        let block = parse_cif_str(
            "data_test
_cell_length_a 4.0
_cell_length_b 4.0
_cell_length_c 4.0
_cell_angle_alpha 90
_cell_angle_beta 90
_cell_angle_gamma 90
loop_
_space_group_symop_operation_xyz
'x, y, z'
'-x, -y, -z'
loop_
_atom_site_type_symbol
_atom_site_fract_x
_atom_site_fract_y
_atom_site_fract_z
Mg 0.1 0.2 0.3
",
        )
        .unwrap();
        let candidate = candidate_from_cif_block(&block, "expanded").unwrap();

        assert_eq!(candidate.species, vec!["Mg", "Mg"]);
        assert_eq!(candidate.periodic_axes, [true, true, true]);
        let lattice = candidate.lattice.unwrap();
        assert!((lattice[0][0] - 4.0).abs() < 1.0e-12);
        assert!(lattice[0][1].abs() < 1.0e-12);
        assert!(lattice[0][2].abs() < 1.0e-12);
        assert!(lattice[1][0].abs() < 1.0e-12);
        assert!((lattice[1][1] - 4.0).abs() < 1.0e-12);
        assert!(lattice[1][2].abs() < 1.0e-12);
        assert!(lattice[2][0].abs() < 1.0e-12);
        assert!(lattice[2][1].abs() < 1.0e-12);
        assert!((lattice[2][2] - 4.0).abs() < 1.0e-12);
        assert!((candidate.fractional_coords[1][0] - 0.9).abs() < 1.0e-12);
        assert!((candidate.fractional_coords[1][1] - 0.8).abs() < 1.0e-12);
        assert!((candidate.fractional_coords[1][2] - 0.7).abs() < 1.0e-12);
    }

    #[test]
    fn scientific_default_policy_allows_missing_occupancy_but_blocks_overfilled_disorder() {
        let missing_occupancy = parse_cif_str(
            "data_test
_cell_length_a 4.0
_cell_length_b 5.0
_cell_length_c 6.0
_cell_angle_alpha 90
_cell_angle_beta 90
_cell_angle_gamma 120
loop_
_atom_site_label
_atom_site_fract_x
_atom_site_fract_y
_atom_site_fract_z
Mg1 0.0 0.0 0.0
",
        )
        .unwrap();
        let validation =
            validate_cif_block(&missing_occupancy, &CifValidationPolicy::default()).unwrap();
        assert!(validation
            .warnings
            .iter()
            .any(|warning| warning.inference == Some(InferenceKind::Symmetry)));

        let overfilled = parse_cif_str(
            "data_test
_cell_length_a 4.0
_cell_length_b 4.0
_cell_length_c 4.0
_cell_angle_alpha 90
_cell_angle_beta 90
_cell_angle_gamma 90
loop_
_atom_site_label
_atom_site_type_symbol
_atom_site_fract_x
_atom_site_fract_y
_atom_site_fract_z
_atom_site_occupancy
M1 Mg 0.0 0.0 0.0 0.8
M1 Zn 0.0 0.0 0.0 0.7
",
        )
        .unwrap();
        let error = validate_cif_block(&overfilled, &CifValidationPolicy::default()).unwrap_err();
        assert!(matches!(error, ConversionError::Unsupported { .. }));
    }

    #[test]
    fn strict_policy_requires_declared_symmetry() {
        let block = parse_cif_str(
            "data_test
_cell_length_a 4.0
_cell_length_b 4.0
_cell_length_c 4.0
_cell_angle_alpha 90
_cell_angle_beta 90
_cell_angle_gamma 90
loop_
_atom_site_type_symbol
_atom_site_fract_x
_atom_site_fract_y
_atom_site_fract_z
_atom_site_occupancy
Mg 0.1 0.2 0.3 1.0
",
        )
        .unwrap();
        let error = validate_cif_block(&block, &CifValidationPolicy::strict()).unwrap_err();
        assert!(matches!(error, ConversionError::InferenceRequired { .. }));
    }

    #[test]
    #[cfg(feature = "bridge-patina-types")]
    fn policy_aware_candidate_bridge_carries_validation_warnings() {
        let block = parse_cif_str(
            "data_test
_cell_length_a 4.0
_cell_length_b 5.0
_cell_length_c 6.0
_cell_angle_alpha 90
_cell_angle_beta 90
_cell_angle_gamma 120
loop_
_atom_site_label
_atom_site_fract_x
_atom_site_fract_y
_atom_site_fract_z
Mg1 0.0 0.0 0.0
",
        )
        .unwrap();

        let outcome =
            candidate_from_cif_block_with_policy(&block, "test", &CifValidationPolicy::default())
                .unwrap();
        assert_eq!(outcome.kind, ConversionKind::RepresentationCodec);
        assert_eq!(outcome.value.periodic_axes, [true, true, true]);
        assert!(!outcome.warnings.is_empty());
    }

    #[test]
    fn geometry_validation_rejects_short_contacts_when_radii_are_available() {
        let block = parse_cif_str(
            "data_test
_cell_length_a 4.0
_cell_length_b 4.0
_cell_length_c 4.0
_cell_angle_alpha 90
_cell_angle_beta 90
_cell_angle_gamma 90
loop_
_space_group_symop_operation_xyz
'x, y, z'
loop_
_atom_site_type_symbol
_atom_site_fract_x
_atom_site_fract_y
_atom_site_fract_z
_atom_site_occupancy
Mg 0.000 0.000 0.000 1.0
O 0.050 0.000 0.000 1.0
",
        )
        .unwrap();
        let error = validate_cif_geometry(
            &block,
            &CifGeometryPolicy::scientific_default(),
            &[
                CifSpeciesRadius {
                    species: "Mg".into(),
                    covalent_radius: 1.41,
                    ionic_radius: 0.72,
                },
                CifSpeciesRadius {
                    species: "O".into(),
                    covalent_radius: 0.66,
                    ionic_radius: 1.26,
                },
            ],
        )
        .unwrap_err();
        assert!(matches!(error, ConversionError::Unsupported { .. }));
    }

    #[test]
    fn geometry_validation_rejects_duplicate_sites_even_without_radii() {
        let block = parse_cif_str(
            "data_test
_cell_length_a 4.0
_cell_length_b 4.0
_cell_length_c 4.0
_cell_angle_alpha 90
_cell_angle_beta 90
_cell_angle_gamma 90
loop_
_space_group_symop_operation_xyz
'x, y, z'
loop_
_atom_site_label
_atom_site_type_symbol
_atom_site_fract_x
_atom_site_fract_y
_atom_site_fract_z
_atom_site_occupancy
A1 Mg 0.000000 0.000000 0.000000 1.0
B1 O 0.000000 0.000000 0.000000 1.0
",
        )
        .unwrap();
        let error = validate_cif_geometry(&block, &CifGeometryPolicy::scientific_default(), &[])
            .unwrap_err();
        assert!(matches!(error, ConversionError::Unsupported { .. }));
    }

    #[test]
    fn scientific_use_validation_combines_semantic_and_geometry_warnings() {
        let block = parse_cif_str(
            "data_test
_cell_length_a 5.0
_cell_length_b 5.0
_cell_length_c 5.0
_cell_angle_alpha 90
_cell_angle_beta 90
_cell_angle_gamma 90
loop_
_atom_site_label
_atom_site_type_symbol
_atom_site_fract_x
_atom_site_fract_y
_atom_site_fract_z
_atom_site_occupancy
X1 Xx 0.0 0.0 0.0 1.0
O1 O 0.5 0.5 0.5 1.0
",
        )
        .unwrap();
        let outcome = validate_cif_for_scientific_use(
            &block,
            &CifScientificPolicy::scientific_default(),
            &[],
        )
        .unwrap();
        assert!(outcome.warnings.len() >= 2);
    }

    #[test]
    #[cfg(feature = "bridge-patina-types")]
    fn scientific_use_bridge_propagates_geometry_validation() {
        let block = parse_cif_str(
            "data_test
_cell_length_a 4.0
_cell_length_b 4.0
_cell_length_c 4.0
_cell_angle_alpha 90
_cell_angle_beta 90
_cell_angle_gamma 90
loop_
_space_group_symop_operation_xyz
'x, y, z'
loop_
_atom_site_type_symbol
_atom_site_fract_x
_atom_site_fract_y
_atom_site_fract_z
_atom_site_occupancy
Mg 0.000 0.000 0.000 1.0
O 0.050 0.000 0.000 1.0
",
        )
        .unwrap();
        let error = candidate_from_cif_block_for_scientific_use(
            &block,
            "test",
            &CifScientificPolicy::scientific_default(),
            &[
                CifSpeciesRadius {
                    species: "Mg".into(),
                    covalent_radius: 1.41,
                    ionic_radius: 0.72,
                },
                CifSpeciesRadius {
                    species: "O".into(),
                    covalent_radius: 0.66,
                    ionic_radius: 1.26,
                },
            ],
        )
        .unwrap_err();
        assert!(matches!(
            error,
            CifBridgeError::Conversion(ConversionError::Unsupported { .. })
        ));
    }
}
