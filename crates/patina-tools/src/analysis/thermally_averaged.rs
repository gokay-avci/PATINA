use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use thiserror::Error;

const BOLTZMANN_EV_PER_K: f64 = 8.617_333_262_145e-5;

#[derive(Debug, Error)]
pub enum ThermalAverageError {
    #[error("at least one data record is required")]
    EmptyRecords,
    #[error("at least one temperature is required")]
    EmptyTemperatures,
    #[error("record {index} has non-finite energy {energy}")]
    InvalidEnergy { index: usize, energy: f64 },
    #[error("record {index} has non-finite property value {value}")]
    InvalidProperty { index: usize, value: f64 },
    #[error("record {index} must have occurrences >= 1")]
    InvalidOccurrences { index: usize },
    #[error("temperature {index} must be positive and finite, received {temperature}")]
    InvalidTemperature { index: usize, temperature: f64 },
    #[error("failed to read thermal-average csv `{path}`: {source}")]
    ReadCsv {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error("thermal-average csv `{path}` contained no rows")]
    EmptyCsv { path: PathBuf },
    #[error(
        "thermal-average csv `{path}` row {row} must contain energy, property, and optional occurrences columns"
    )]
    InvalidCsvRow { path: PathBuf, row: usize },
    #[error("thermal-average csv `{path}` row {row} had invalid numeric value `{value}`")]
    InvalidCsvValue {
        path: PathBuf,
        row: usize,
        value: String,
    },
}

impl PartialEq for ThermalAverageError {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (Self::EmptyRecords, Self::EmptyRecords) => true,
            (Self::EmptyTemperatures, Self::EmptyTemperatures) => true,
            (
                Self::InvalidEnergy {
                    index: left_index,
                    energy: left_energy,
                },
                Self::InvalidEnergy {
                    index: right_index,
                    energy: right_energy,
                },
            ) => left_index == right_index && left_energy == right_energy,
            (
                Self::InvalidProperty {
                    index: left_index,
                    value: left_value,
                },
                Self::InvalidProperty {
                    index: right_index,
                    value: right_value,
                },
            ) => left_index == right_index && left_value == right_value,
            (
                Self::InvalidOccurrences { index: left_index },
                Self::InvalidOccurrences { index: right_index },
            ) => left_index == right_index,
            (
                Self::InvalidTemperature {
                    index: left_index,
                    temperature: left_temperature,
                },
                Self::InvalidTemperature {
                    index: right_index,
                    temperature: right_temperature,
                },
            ) => left_index == right_index && left_temperature == right_temperature,
            (Self::ReadCsv { path: left, .. }, Self::ReadCsv { path: right, .. }) => left == right,
            (Self::EmptyCsv { path: left }, Self::EmptyCsv { path: right }) => left == right,
            (
                Self::InvalidCsvRow {
                    path: left_path,
                    row: left_row,
                },
                Self::InvalidCsvRow {
                    path: right_path,
                    row: right_row,
                },
            ) => left_path == right_path && left_row == right_row,
            (
                Self::InvalidCsvValue {
                    path: left_path,
                    row: left_row,
                    value: left_value,
                },
                Self::InvalidCsvValue {
                    path: right_path,
                    row: right_row,
                    value: right_value,
                },
            ) => left_path == right_path && left_row == right_row && left_value == right_value,
            _ => false,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ThermallyAveragedRecord {
    pub energy: f64,
    pub property: f64,
    pub occurrences: usize,
}

impl ThermallyAveragedRecord {
    pub fn unweighted(energy: f64, property: f64) -> Self {
        Self {
            energy,
            property,
            occurrences: 1,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ThermalAveragePoint {
    pub temperature_kelvin: f64,
    pub value: f64,
    pub partition_function: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ThermalAverageWorkflowReport {
    pub input_path: PathBuf,
    pub record_count: usize,
    pub use_occurrences: bool,
    pub points: Vec<ThermalAveragePoint>,
}

pub fn parse_temperature_list(input: &str) -> Result<Vec<f64>, ThermalAverageError> {
    let temperatures = input
        .split(',')
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(|value| value.parse::<f64>().unwrap_or(f64::NAN))
        .collect::<Vec<_>>();

    compute_temperature_validity(&temperatures)?;

    Ok(temperatures)
}

pub fn compute_thermally_averaged_statistics(
    records: &[ThermallyAveragedRecord],
    temperatures_kelvin: &[f64],
    use_occurrences: bool,
) -> Result<Vec<ThermalAveragePoint>, ThermalAverageError> {
    validate_records(records)?;
    compute_temperature_validity(temperatures_kelvin)?;

    let min_energy = records
        .iter()
        .map(|record| record.energy)
        .fold(f64::INFINITY, f64::min);

    let mut results = Vec::with_capacity(temperatures_kelvin.len());
    for &temperature_kelvin in temperatures_kelvin {
        let kt = BOLTZMANN_EV_PER_K * temperature_kelvin;
        let mut numerator = 0.0;
        let mut denominator = 0.0;

        for record in records {
            let multiplicity = if use_occurrences {
                record.occurrences as f64
            } else {
                1.0
            };
            let boltzmann = (-(record.energy - min_energy) / kt).exp() * multiplicity;
            numerator += record.property * boltzmann;
            denominator += boltzmann;
        }

        results.push(ThermalAveragePoint {
            temperature_kelvin,
            value: numerator / denominator,
            partition_function: denominator,
        });
    }

    Ok(results)
}

pub fn load_thermal_average_records_csv(
    path: &Path,
) -> Result<Vec<ThermallyAveragedRecord>, ThermalAverageError> {
    let raw = std::fs::read_to_string(path).map_err(|source| ThermalAverageError::ReadCsv {
        path: path.to_path_buf(),
        source,
    })?;

    let mut records = Vec::new();
    for (index, line) in raw.lines().enumerate() {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        if index == 0 && trimmed.to_ascii_lowercase().contains("energy") {
            continue;
        }

        let parts = trimmed.split(',').map(str::trim).collect::<Vec<_>>();
        if parts.len() < 2 || parts.len() > 3 {
            return Err(ThermalAverageError::InvalidCsvRow {
                path: path.to_path_buf(),
                row: index + 1,
            });
        }

        let energy = parse_csv_f64(path, index + 1, parts[0])?;
        let property = parse_csv_f64(path, index + 1, parts[1])?;
        let occurrences = if parts.len() == 3 {
            let parsed = parse_csv_usize(path, index + 1, parts[2])?;
            parsed.max(1)
        } else {
            1
        };

        records.push(ThermallyAveragedRecord {
            energy,
            property,
            occurrences,
        });
    }

    if records.is_empty() {
        return Err(ThermalAverageError::EmptyCsv {
            path: path.to_path_buf(),
        });
    }

    Ok(records)
}

pub fn run_thermally_averaged_workflow(
    input_path: &Path,
    temperatures_kelvin: &[f64],
    use_occurrences: bool,
) -> Result<ThermalAverageWorkflowReport, ThermalAverageError> {
    let records = load_thermal_average_records_csv(input_path)?;
    let points =
        compute_thermally_averaged_statistics(&records, temperatures_kelvin, use_occurrences)?;
    Ok(ThermalAverageWorkflowReport {
        input_path: input_path.to_path_buf(),
        record_count: records.len(),
        use_occurrences,
        points,
    })
}

fn validate_records(records: &[ThermallyAveragedRecord]) -> Result<(), ThermalAverageError> {
    if records.is_empty() {
        return Err(ThermalAverageError::EmptyRecords);
    }

    for (index, record) in records.iter().enumerate() {
        if !record.energy.is_finite() {
            return Err(ThermalAverageError::InvalidEnergy {
                index,
                energy: record.energy,
            });
        }
        if !record.property.is_finite() {
            return Err(ThermalAverageError::InvalidProperty {
                index,
                value: record.property,
            });
        }
        if record.occurrences == 0 {
            return Err(ThermalAverageError::InvalidOccurrences { index });
        }
    }

    Ok(())
}

fn compute_temperature_validity(temperatures_kelvin: &[f64]) -> Result<(), ThermalAverageError> {
    if temperatures_kelvin.is_empty() {
        return Err(ThermalAverageError::EmptyTemperatures);
    }

    for (index, &temperature) in temperatures_kelvin.iter().enumerate() {
        if !temperature.is_finite() || temperature <= 0.0 {
            return Err(ThermalAverageError::InvalidTemperature { index, temperature });
        }
    }

    Ok(())
}

fn parse_csv_f64(path: &Path, row: usize, value: &str) -> Result<f64, ThermalAverageError> {
    value
        .parse::<f64>()
        .map_err(|_| ThermalAverageError::InvalidCsvValue {
            path: path.to_path_buf(),
            row,
            value: value.to_string(),
        })
}

fn parse_csv_usize(path: &Path, row: usize, value: &str) -> Result<usize, ThermalAverageError> {
    value
        .parse::<usize>()
        .map_err(|_| ThermalAverageError::InvalidCsvValue {
            path: path.to_path_buf(),
            row,
            value: value.to_string(),
        })
}

#[cfg(test)]
mod tests {
    use super::{
        compute_thermally_averaged_statistics, load_thermal_average_records_csv,
        parse_temperature_list, run_thermally_averaged_workflow, ThermalAverageError,
        ThermallyAveragedRecord,
    };
    use std::fs;
    use tempfile::tempdir;

    #[test]
    fn parses_comma_separated_temperature_list() {
        let temperatures = parse_temperature_list("293, 500, 1000").expect("parse temperatures");
        assert_eq!(temperatures, vec![293.0, 500.0, 1000.0]);
    }

    #[test]
    fn computes_unweighted_canonical_average() {
        let records = vec![
            ThermallyAveragedRecord::unweighted(0.0, 1.0),
            ThermallyAveragedRecord::unweighted(0.1, 3.0),
        ];

        let points = compute_thermally_averaged_statistics(&records, &[1_000.0], false)
            .expect("compute averages");

        assert_eq!(points.len(), 1);
        assert!(points[0].value > 1.0);
        assert!(points[0].value < 3.0);
    }

    #[test]
    fn occurrences_shift_the_average_towards_more_frequent_records() {
        let records = vec![
            ThermallyAveragedRecord {
                energy: 0.0,
                property: 1.0,
                occurrences: 1,
            },
            ThermallyAveragedRecord {
                energy: 0.0,
                property: 3.0,
                occurrences: 3,
            },
        ];

        let points = compute_thermally_averaged_statistics(&records, &[293.0], true)
            .expect("compute weighted averages");

        assert!((points[0].value - 2.5).abs() < 1.0e-12);
    }

    #[test]
    fn rejects_non_positive_temperature() {
        let records = vec![ThermallyAveragedRecord::unweighted(0.0, 1.0)];
        let error = compute_thermally_averaged_statistics(&records, &[0.0], false)
            .expect_err("temperature validation");

        assert_eq!(
            error,
            ThermalAverageError::InvalidTemperature {
                index: 0,
                temperature: 0.0,
            }
        );
    }

    #[test]
    fn loads_csv_records_with_optional_occurrences() {
        let dir = tempdir().expect("tempdir");
        let path = dir.path().join("thermal.csv");
        fs::write(&path, "energy,property,occurrences\n0.0,1.0,2\n0.1,3.0,4\n").expect("write csv");

        let records = load_thermal_average_records_csv(&path).expect("load csv");
        assert_eq!(records.len(), 2);
        assert_eq!(records[0].occurrences, 2);
        assert_eq!(records[1].property, 3.0);
    }

    #[test]
    fn runs_workflow_from_csv_input() {
        let dir = tempdir().expect("tempdir");
        let path = dir.path().join("thermal.csv");
        fs::write(&path, "energy,property\n0.0,1.0\n0.1,3.0\n").expect("write csv");

        let report =
            run_thermally_averaged_workflow(&path, &[293.0, 500.0], false).expect("workflow");
        assert_eq!(report.record_count, 2);
        assert_eq!(report.points.len(), 2);
    }
}
