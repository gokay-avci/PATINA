use serde::{Deserialize, Serialize};

/// Broad families of structure conversion.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ConversionKind {
    StructureValidation,
    ScientificTransform,
    RepresentationCodec,
    Bridge,
}

/// Meaning inferred during a conversion.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum InferenceKind {
    Periodicity,
    CoordinateBasis,
    Lattice,
    Dimensionality,
    Symmetry,
}

/// Meaning lost or intentionally discarded during a conversion.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LossKind {
    MetadataDropped,
    SymmetryDropped,
    LatticeDropped,
    PeriodicityDropped,
    DialectSpecificDataDropped,
}

/// Non-fatal warning returned by a conversion.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ConversionWarning {
    pub message: String,
    pub inference: Option<InferenceKind>,
    pub loss: Option<LossKind>,
}

impl ConversionWarning {
    pub fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
            inference: None,
            loss: None,
        }
    }
}

/// Shared conversion policy scaffold for future richer transforms and codecs.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct ConversionPolicy {
    pub allow_inference: bool,
    pub allow_lossy: bool,
}

impl Default for ConversionPolicy {
    fn default() -> Self {
        Self {
            allow_inference: true,
            allow_lossy: false,
        }
    }
}

/// Successful conversion plus any non-fatal warnings.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ConversionOutcome<T> {
    pub kind: ConversionKind,
    pub value: T,
    pub warnings: Vec<ConversionWarning>,
}

impl<T> ConversionOutcome<T> {
    pub fn new(kind: ConversionKind, value: T) -> Self {
        Self {
            kind,
            value,
            warnings: Vec::new(),
        }
    }

    pub fn with_warning(mut self, warning: ConversionWarning) -> Self {
        self.warnings.push(warning);
        self
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ConversionError {
    InferenceRequired { message: String },
    LossyConversionDisallowed { message: String },
    Unsupported { message: String },
}
