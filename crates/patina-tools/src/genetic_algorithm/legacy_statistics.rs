use serde::{Deserialize, Serialize};
use thiserror::Error;

#[derive(Debug, Error, PartialEq)]
pub enum LegacyGaStatisticsError {
    #[error(
        "legacy GA statistics row must have at least 16 comma-separated columns, got {columns}"
    )]
    TooFewColumns { columns: usize },
    #[error("failed to parse `{field}` value `{value}`")]
    InvalidField { field: &'static str, value: String },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LegacyGaStatisticsRow {
    pub rank: usize,
    pub cluster_id: String,
    pub hashkey: String,
    pub energy: f64,
    pub parent1: Option<String>,
    pub parent2: Option<String>,
    pub origin: String,
    pub generation: usize,
}

pub fn parse_legacy_ga_statistics_row(
    line: &str,
) -> Result<LegacyGaStatisticsRow, LegacyGaStatisticsError> {
    let columns = line.split(',').map(str::trim).collect::<Vec<_>>();
    if columns.len() < 16 {
        return Err(LegacyGaStatisticsError::TooFewColumns {
            columns: columns.len(),
        });
    }

    Ok(LegacyGaStatisticsRow {
        rank: parse_usize(columns[0], "rank")?,
        cluster_id: columns[1].to_string(),
        hashkey: columns[2].to_string(),
        energy: parse_f64(columns[5], "energy")?,
        parent1: parse_optional_string(columns[8]),
        parent2: parse_optional_string(columns[9]),
        origin: columns[14].to_string(),
        generation: parse_usize(columns[15], "generation")?,
    })
}

pub fn parse_legacy_ga_statistics_contents(
    contents: &str,
) -> Result<Vec<LegacyGaStatisticsRow>, LegacyGaStatisticsError> {
    let mut rows = Vec::new();
    for (index, line) in contents.lines().enumerate() {
        if index == 0 || line.trim().is_empty() {
            continue;
        }
        rows.push(parse_legacy_ga_statistics_row(line)?);
    }
    Ok(rows)
}

pub fn best_member_from_legacy_statistics(
    contents: &str,
) -> Result<Option<LegacyGaStatisticsRow>, LegacyGaStatisticsError> {
    let rows = parse_legacy_ga_statistics_contents(contents)?;
    Ok(rows
        .into_iter()
        .min_by(|left, right| left.energy.total_cmp(&right.energy)))
}

pub fn find_member_by_hashkey(
    contents: &str,
    hashkey: &str,
) -> Result<Option<LegacyGaStatisticsRow>, LegacyGaStatisticsError> {
    let rows = parse_legacy_ga_statistics_contents(contents)?;
    Ok(rows.into_iter().find(|row| row.hashkey == hashkey))
}

fn parse_optional_string(value: &str) -> Option<String> {
    let trimmed = value.trim();
    if trimmed.is_empty() || trimmed.eq_ignore_ascii_case("none") {
        None
    } else {
        Some(trimmed.to_string())
    }
}

fn parse_usize(value: &str, field: &'static str) -> Result<usize, LegacyGaStatisticsError> {
    value
        .parse::<usize>()
        .map_err(|_| LegacyGaStatisticsError::InvalidField {
            field,
            value: value.to_string(),
        })
}

fn parse_f64(value: &str, field: &'static str) -> Result<f64, LegacyGaStatisticsError> {
    value
        .parse::<f64>()
        .map_err(|_| LegacyGaStatisticsError::InvalidField {
            field,
            value: value.to_string(),
        })
}

#[cfg(test)]
mod tests {
    use super::{
        best_member_from_legacy_statistics, find_member_by_hashkey,
        parse_legacy_ga_statistics_contents, parse_legacy_ga_statistics_row,
    };

    const SAMPLE: &str =
        "rank,name,hashkey,status,edefn,energy,a,b,parent1,parent2,c,d,e,f,origin,generation\n\
1,A001,h1,1,1,-10.5,0,0,P1,P2,0,0,0,0,CROSSO,12\n\
2,A002,h2,1,1,-9.0,0,0,,,0,0,0,0,REPOPM,12\n";

    #[test]
    fn parses_legacy_statistics_row() {
        let row = parse_legacy_ga_statistics_row("1,A001,h1,1,1,-10.5,0,0,P1,P2,0,0,0,0,CROSSO,12")
            .expect("parse row");

        assert_eq!(row.rank, 1);
        assert_eq!(row.cluster_id, "A001");
        assert_eq!(row.hashkey, "h1");
        assert_eq!(row.energy, -10.5);
        assert_eq!(row.parent1.as_deref(), Some("P1"));
        assert_eq!(row.parent2.as_deref(), Some("P2"));
        assert_eq!(row.origin, "CROSSO");
        assert_eq!(row.generation, 12);
    }

    #[test]
    fn parses_contents_and_skips_header() {
        let rows = parse_legacy_ga_statistics_contents(SAMPLE).expect("parse contents");
        assert_eq!(rows.len(), 2);
    }

    #[test]
    fn finds_best_member_by_energy() {
        let best = best_member_from_legacy_statistics(SAMPLE)
            .expect("best member")
            .expect("present");
        assert_eq!(best.hashkey, "h1");
    }

    #[test]
    fn finds_member_by_hashkey() {
        let row = find_member_by_hashkey(SAMPLE, "h2")
            .expect("find member")
            .expect("present");
        assert_eq!(row.cluster_id, "A002");
        assert_eq!(row.origin, "REPOPM");
    }
}
