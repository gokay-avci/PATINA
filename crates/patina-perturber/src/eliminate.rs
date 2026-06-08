use crate::structure::ClusterStructure;
use crate::PerturberError;

pub fn eliminate_by_threshold(
    structures: &[ClusterStructure],
    distance_matrix: &[Vec<f64>],
    threshold: f64,
) -> Result<Vec<ClusterStructure>, PerturberError> {
    if threshold < 0.0 || !threshold.is_finite() {
        return Err(PerturberError::InvalidCandidate(
            "duplicate-screening threshold must be finite and >= 0".into(),
        ));
    }
    if structures.len() != distance_matrix.len() {
        return Err(PerturberError::InvalidCandidate(
            "distance matrix size does not match structure count".into(),
        ));
    }
    for (row_index, row) in distance_matrix.iter().enumerate() {
        if row.len() != structures.len() {
            return Err(PerturberError::InvalidCandidate(format!(
                "distance matrix row {row_index} has length {}, expected {}",
                row.len(),
                structures.len()
            )));
        }
        for (col_index, value) in row.iter().enumerate() {
            if !value.is_finite() {
                return Err(PerturberError::InvalidCandidate(format!(
                    "distance matrix has non-finite value at ({row_index},{col_index})"
                )));
            }
            if *value < 0.0 {
                return Err(PerturberError::InvalidCandidate(format!(
                    "distance matrix has negative value at ({row_index},{col_index})"
                )));
            }
        }
    }
    for structure in structures {
        structure.validate()?;
    }
    let mut kept = Vec::<usize>::new();
    for (index, _) in structures.iter().enumerate() {
        if kept
            .iter()
            .all(|kept_index| distance_matrix[index][*kept_index] >= threshold)
        {
            kept.push(index);
        }
    }
    Ok(kept
        .into_iter()
        .map(|index| structures[index].clone())
        .collect())
}
