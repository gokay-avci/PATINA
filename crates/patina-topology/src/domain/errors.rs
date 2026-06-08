use thiserror::Error;

pub type TopologyResult<T> = Result<T, TopologyError>;

#[derive(Debug, Error)]
pub enum TopologyError {
    #[error("empty formula")]
    EmptyFormula,
    #[error("invalid formula near byte {index}: `{formula}`")]
    InvalidFormula { formula: String, index: usize },
    #[error("formula unit count must be greater than zero")]
    InvalidFormulaUnitCount,
    #[error("motif requires at least {minimum} atoms, got {actual}")]
    TooFewAtoms { minimum: usize, actual: usize },
    #[error("invalid bond indices ({i}, {j}) for {n_atoms} atoms")]
    InvalidBondIndex { i: usize, j: usize, n_atoms: usize },
    #[error("candidate `{id}` has disconnected graph")]
    DisconnectedGraph { id: String },
    #[error("I/O error: {message}")]
    Io { message: String },
    #[error("JSON error: {message}")]
    Json { message: String },
    #[error("unsupported generator `{name}`")]
    UnsupportedGenerator { name: String },
    #[error("unsupported bond-length mode `{mode}`")]
    UnsupportedBondLengthMode { mode: String },
    #[error("backend `{backend}` failed: {message}")]
    Backend { backend: String, message: String },
}

impl From<std::io::Error> for TopologyError {
    fn from(value: std::io::Error) -> Self {
        Self::Io {
            message: value.to_string(),
        }
    }
}

impl From<serde_json::Error> for TopologyError {
    fn from(value: serde_json::Error) -> Self {
        Self::Json {
            message: value.to_string(),
        }
    }
}
