/*!
Shared domain errors and crate-level conventions for `patina-stk`.
*/

use thiserror::Error;

#[derive(Debug, Error, PartialEq, Eq)]
pub enum StkDomainError {
    #[error("duplicate topology vertex id `{id}`")]
    DuplicateVertexId { id: usize },
    #[error("duplicate topology edge id `{id}`")]
    DuplicateEdgeId { id: usize },
    #[error("topology edge `{edge_id}` references missing vertex `{vertex_id}`")]
    MissingVertexReference { edge_id: usize, vertex_id: usize },
    #[error("topology graph is missing edge-group coverage")]
    MissingEdgeGroupCoverage,
    #[error("topology edge-group references missing edge `{edge_id}`")]
    MissingEdgeGroupEdge { edge_id: usize },
    #[error("topology edge-group contains duplicate edge `{edge_id}`")]
    DuplicateEdgeGroupEdge { edge_id: usize },
    #[error("periodic edge `{edge_id}` requires lattice vectors")]
    MissingLatticeForPeriodicEdge { edge_id: usize },
    #[error("topology vertex `{vertex_id}` was not found")]
    VertexNotFound { vertex_id: usize },
    #[error("topology edge `{edge_id}` was not found")]
    EdgeNotFound { edge_id: usize },
    #[error("invalid placement request: {reason}")]
    InvalidPlacement { reason: &'static str },
    #[error("invalid building-block preparation: {reason}")]
    InvalidBuildingBlock { reason: &'static str },
    #[error("python chemistry adapter failed: {reason}")]
    PythonAdapter { reason: String },
}
