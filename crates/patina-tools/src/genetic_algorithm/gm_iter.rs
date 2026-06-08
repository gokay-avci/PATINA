use crate::genetic_algorithm::legacy_statistics::{
    best_member_from_legacy_statistics, LegacyGaStatisticsError, LegacyGaStatisticsRow,
};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum GmIterError {
    #[error("run root `{0}` does not exist")]
    MissingRunRoot(PathBuf),
    #[error("failed to read GA statistics `{path}`: {source}")]
    ReadStatistics {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error("failed to parse GA statistics `{path}`: {source}")]
    ParseStatistics {
        path: PathBuf,
        #[source]
        source: LegacyGaStatisticsError,
    },
    #[error("no GA statistics files were found under `{0}`")]
    NoStatisticsFiles(PathBuf),
    #[error("could not determine a best member from GA statistics")]
    NoBestMember,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GmIterConfig {
    pub run_root: PathBuf,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GmIterReport {
    pub latest_generation: usize,
    pub first_matching_generation: usize,
    pub final_best_hashkey: String,
    pub final_best_energy: f64,
    pub final_best_cluster_id: String,
    pub final_best_origin: String,
    pub scanned_file_count: usize,
}

pub fn run_gm_iter_workflow(config: &GmIterConfig) -> Result<GmIterReport, GmIterError> {
    let files = collect_ga_statistics_files(&config.run_root)?;
    if files.is_empty() {
        return Err(GmIterError::NoStatisticsFiles(config.run_root.clone()));
    }

    let mut best_by_generation = Vec::<(usize, LegacyGaStatisticsRow)>::new();
    for path in &files {
        let raw = fs::read_to_string(path).map_err(|source| GmIterError::ReadStatistics {
            path: path.clone(),
            source,
        })?;
        if let Some(best) = best_member_from_legacy_statistics(&raw).map_err(|source| {
            GmIterError::ParseStatistics {
                path: path.clone(),
                source,
            }
        })? {
            let generation = generation_from_filename(path).unwrap_or(best.generation);
            best_by_generation.push((generation, best));
        }
    }

    if best_by_generation.is_empty() {
        return Err(GmIterError::NoBestMember);
    }

    best_by_generation.sort_by_key(|(generation, _)| *generation);
    let (latest_generation, final_best) = best_by_generation
        .last()
        .cloned()
        .ok_or(GmIterError::NoBestMember)?;

    let first_matching_generation = best_by_generation
        .iter()
        .find(|(_, row)| row.hashkey == final_best.hashkey)
        .map(|(generation, _)| *generation)
        .ok_or(GmIterError::NoBestMember)?;

    Ok(GmIterReport {
        latest_generation,
        first_matching_generation,
        final_best_hashkey: final_best.hashkey,
        final_best_energy: final_best.energy,
        final_best_cluster_id: final_best.cluster_id,
        final_best_origin: final_best.origin,
        scanned_file_count: files.len(),
    })
}

fn collect_ga_statistics_files(run_root: &Path) -> Result<Vec<PathBuf>, GmIterError> {
    if !run_root.exists() {
        return Err(GmIterError::MissingRunRoot(run_root.to_path_buf()));
    }

    let mut files = Vec::new();
    collect_ga_statistics_files_inner(run_root, &mut files)?;
    files.sort_by_key(|path| generation_from_filename(path).unwrap_or(usize::MAX));
    Ok(files)
}

fn collect_ga_statistics_files_inner(
    current: &Path,
    files: &mut Vec<PathBuf>,
) -> Result<(), GmIterError> {
    for entry in
        fs::read_dir(current).map_err(|_| GmIterError::MissingRunRoot(current.to_path_buf()))?
    {
        let entry = entry.map_err(|_| GmIterError::MissingRunRoot(current.to_path_buf()))?;
        let path = entry.path();
        if path.is_dir() {
            collect_ga_statistics_files_inner(&path, files)?;
            continue;
        }
        let matches = path
            .file_name()
            .and_then(|name| name.to_str())
            .map(|name| name.starts_with("gaStatistics") && name.ends_with(".csv"))
            .unwrap_or(false);
        if matches {
            files.push(path);
        }
    }

    Ok(())
}

fn generation_from_filename(path: &Path) -> Option<usize> {
    let stem = path.file_stem()?.to_str()?;
    let suffix = stem.strip_prefix("gaStatistics")?;
    suffix.parse::<usize>().ok()
}

#[cfg(test)]
mod tests {
    use super::{run_gm_iter_workflow, GmIterConfig};
    use std::fs;
    use tempfile::tempdir;

    fn row(rank: usize, hashkey: &str, energy: f64, generation: usize) -> String {
        format!("{rank},A{rank:03},{hashkey},1,1,{energy},0,0,P1,P2,0,0,0,0,CROSSO,{generation}\n")
    }

    #[test]
    fn finds_first_generation_where_final_gm_hashkey_appears() {
        let dir = tempdir().expect("tempdir");
        let run = dir.path().join("run");
        fs::create_dir_all(&run).expect("run dir");

        fs::write(
            run.join("gaStatistics0.csv"),
            format!(
                "rank,name,hashkey,status,edefn,energy,a,b,parent1,parent2,c,d,e,f,origin,generation\n{}",
                row(1, "h0", -1.0, 0)
            ),
        )
        .expect("write g0");
        fs::write(
            run.join("gaStatistics1.csv"),
            format!(
                "rank,name,hashkey,status,edefn,energy,a,b,parent1,parent2,c,d,e,f,origin,generation\n{}",
                row(1, "hgm", -2.0, 1)
            ),
        )
        .expect("write g1");
        fs::write(
            run.join("gaStatistics2.csv"),
            format!(
                "rank,name,hashkey,status,edefn,energy,a,b,parent1,parent2,c,d,e,f,origin,generation\n{}",
                row(1, "hgm", -2.5, 2)
            ),
        )
        .expect("write g2");

        let report = run_gm_iter_workflow(&GmIterConfig {
            run_root: run.clone(),
        })
        .expect("gm iter workflow");

        assert_eq!(report.latest_generation, 2);
        assert_eq!(report.first_matching_generation, 1);
        assert_eq!(report.final_best_hashkey, "hgm");
        assert_eq!(report.final_best_energy, -2.5);
    }
}
