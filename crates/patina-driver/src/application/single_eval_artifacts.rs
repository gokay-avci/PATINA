use anyhow::{Context, Result};
use serde::Serialize;
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

use super::driver_support::EvalBackendKind;

#[derive(Debug, Clone, Serialize)]
pub struct SingleEvalArtifactArgs {
    pub workdir: PathBuf,
    pub mode: String,
    pub system: String,
    pub engine: String,
    pub backend: EvalBackendKind,
    pub backend_metadata: Option<serde_json::Value>,
    pub procedure_state_digest: Option<patina_evaluator::ScottStateDigest>,
    pub failure_reason: Option<String>,
}

pub fn write_single_eval_artifacts(
    run_dir: &Path,
    args: &SingleEvalArtifactArgs,
    candidate: &patina_types::Candidate,
    result: Option<&patina_types::EvalResult>,
) -> Result<()> {
    let traces_dir = run_dir.join("traces");
    let outputs_dir = run_dir.join("outputs");
    let raw_dir = run_dir.join("raw");
    let structures_dir = outputs_dir.join("structures");
    fs::create_dir_all(&traces_dir).with_context(|| {
        format!(
            "failed to create traces directory `{}`",
            traces_dir.display()
        )
    })?;
    fs::create_dir_all(&outputs_dir).with_context(|| {
        format!(
            "failed to create outputs directory `{}`",
            outputs_dir.display()
        )
    })?;
    fs::create_dir_all(&raw_dir)
        .with_context(|| format!("failed to create raw directory `{}`", raw_dir.display()))?;
    fs::create_dir_all(&structures_dir).with_context(|| {
        format!(
            "failed to create structures directory `{}`",
            structures_dir.display()
        )
    })?;

    let run_name = run_dir
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("unnamed-run");
    let run_status = if result.is_some() {
        "success"
    } else {
        "failure"
    };
    let manifest = serde_json::json!({
        "run_name": run_name,
        "status": run_status,
        "mode": args.mode,
        "system": args.system,
        "engine": args.engine,
        "backend": args.backend,
        "backend_metadata": args.backend_metadata,
        "procedure_state_digest": args.procedure_state_digest,
        "failure_reason": args.failure_reason,
        "notes": format!("Single-candidate backend evaluation exported by patina-driver for candidate {}", candidate.label),
        "walker_trace": "traces/walker_trace.csv",
        "mc_trace": "traces/mc_trace.csv",
        "generation_metrics": "traces/generation_metrics.csv"
    });
    fs::write(
        run_dir.join("manifest.json"),
        serde_json::to_string_pretty(&manifest).context("failed to serialize run manifest")?,
    )
    .with_context(|| format!("failed to write manifest in `{}`", run_dir.display()))?;

    if let Some(result) = result {
        fs::write(
            outputs_dir.join("eval_result.json"),
            serde_json::to_string_pretty(result).context("failed to serialize EvalResult")?,
        )
        .with_context(|| {
            format!(
                "failed to write eval_result.json in `{}`",
                outputs_dir.display()
            )
        })?;
    }

    fs::write(
        raw_dir.join("candidate.json"),
        serde_json::to_string_pretty(candidate).context("failed to serialize Candidate")?,
    )
    .with_context(|| format!("failed to write candidate.json in `{}`", raw_dir.display()))?;

    if let Some(digest) = args.procedure_state_digest.as_ref() {
        fs::write(
            raw_dir.join("procedure_state_digest.json"),
            serde_json::to_string_pretty(digest)
                .context("failed to serialize Scott procedure state digest")?,
        )
        .with_context(|| {
            format!(
                "failed to write procedure_state_digest.json in `{}`",
                raw_dir.display()
            )
        })?;
    }

    fs::write(
        raw_dir.join("evaluation_status.json"),
        serde_json::to_string_pretty(&serde_json::json!({
            "status": run_status,
            "failure_reason": args.failure_reason,
        }))
        .context("failed to serialize evaluation status")?,
    )
    .with_context(|| {
        format!(
            "failed to write evaluation_status.json in `{}`",
            raw_dir.display()
        )
    })?;

    copy_backend_raw_outputs(args, &raw_dir, &structures_dir)?;

    if let Some(result) = result {
        if args.mode.eq_ignore_ascii_case("bh") {
            let path = traces_dir.join("walker_trace.csv");
            let mut file = fs::File::create(&path)
                .with_context(|| format!("failed to create `{}`", path.display()))?;
            writeln!(
                file,
                "step,walker_id,accepted,energy,best_energy,temperature,step_size,move_class,label"
            )
            .context("failed to write walker trace header")?;
            writeln!(
                file,
                "0,0,{}, {:.10},{:.10},0.0,0.0,scott_evaluate,{}",
                result.converged, result.energy, result.energy, candidate.label
            )
            .context("failed to write walker trace row")?;
        } else if is_mc_family_mode(&args.mode) {
            let path = traces_dir.join("mc_trace.csv");
            let mut file = fs::File::create(&path)
                .with_context(|| format!("failed to create `{}`", path.display()))?;
            writeln!(
                file,
                "step,accepted,energy,best_energy,temperature,threshold,label,reason,energy_source"
            )
            .context("failed to write mc trace header")?;
            writeln!(
                file,
                "0,{}, {:.10},{:.10},0.0,,{},single_eval,rust_eval",
                result.converged, result.energy, result.energy, candidate.label,
            )
            .context("failed to write mc trace row")?;
        } else if args.mode.eq_ignore_ascii_case("ga") {
            let path = traces_dir.join("generation_metrics.csv");
            let mut file = fs::File::create(&path)
                .with_context(|| format!("failed to create `{}`", path.display()))?;
            writeln!(
            file,
            "generation,best_energy,mean_energy,worst_energy,n_converged,n_failed,population_size"
        )
            .context("failed to write generation metrics header")?;
            writeln!(
                file,
                "0,{:.10},{:.10},{:.10},{},0,1",
                result.energy,
                result.energy,
                result.energy,
                usize::from(result.converged)
            )
            .context("failed to write generation metrics row")?;
        }
    }

    Ok(())
}

fn is_mc_family_mode(mode: &str) -> bool {
    mode.eq_ignore_ascii_case("mc")
        || mode.eq_ignore_ascii_case("monte_carlo")
        || mode.eq_ignore_ascii_case("annealing")
        || mode.eq_ignore_ascii_case("simulated_annealing")
        || mode.eq_ignore_ascii_case("energy_lid")
}

#[cfg(test)]
mod tests {
    use super::{write_single_eval_artifacts, EvalBackendKind, SingleEvalArtifactArgs};
    use patina_evaluator::{ScottStateDigest, StageIndex};
    use patina_types::{Candidate, EvalResult};
    use std::fs;
    use std::time::Duration;
    use tempfile::tempdir;

    fn sample_candidate(label: &str) -> Candidate {
        Candidate::cluster(label, vec!["Mg".into()], vec![[0.0, 0.0, 0.0]])
    }

    fn sample_result(label: &str) -> EvalResult {
        EvalResult {
            energy: -1.25,
            forces: vec![[0.0, 0.0, 0.0]],
            relaxed_candidate: sample_candidate(label),
            converged: true,
            wall_time: Duration::from_secs(1),
        }
    }

    #[test]
    fn writes_failure_artifacts_without_eval_result() {
        let dir = tempdir().expect("tempdir");
        let run_dir = dir.path().join("single_eval_failure");
        let workdir = dir.path().join("backend_work");
        fs::create_dir_all(&workdir).expect("workdir");
        let candidate = sample_candidate("seed");
        let digest = ScottStateDigest {
            request_id: "req-1".into(),
            input_label: "seed".into(),
            current_stage: Some(StageIndex::FIRST),
            accepted_stage: None,
            accepted_label: None,
            stage_attempt_count: 2,
            failure_count: 1,
            procedure_gate_count: 1,
            procedure_rejection_count: 1,
            validity_failure_count: 1,
            relax_failed: true,
            rollback_to_stage: Some(StageIndex::FIRST),
            last_failure_message: Some("backend requires more cycles".into()),
            last_procedure_gate_message: Some(
                "stage converged but no relaxed result payload was available".into(),
            ),
            topology_skip: false,
            input_hashkey: None,
            final_hashkey: None,
            duplicate_reason: None,
            duplicate_action: None,
            duplicate_rank: None,
        };
        let args = SingleEvalArtifactArgs {
            workdir,
            mode: "production".into(),
            system: "MgO".into(),
            engine: "Scott staged (Gulp)".into(),
            backend: EvalBackendKind::Gulp,
            backend_metadata: None,
            procedure_state_digest: Some(digest),
            failure_reason: Some("no accepted final result".into()),
        };

        write_single_eval_artifacts(&run_dir, &args, &candidate, None).expect("write artifacts");

        assert!(run_dir.join("manifest.json").exists());
        assert!(run_dir.join("raw").join("candidate.json").exists());
        assert!(run_dir
            .join("raw")
            .join("procedure_state_digest.json")
            .exists());
        assert!(run_dir.join("raw").join("evaluation_status.json").exists());
        assert!(!run_dir.join("outputs").join("eval_result.json").exists());

        let manifest: serde_json::Value = serde_json::from_str(
            &fs::read_to_string(run_dir.join("manifest.json")).expect("read manifest"),
        )
        .expect("parse manifest");
        assert_eq!(manifest["status"], "failure");
        assert_eq!(manifest["failure_reason"], "no accepted final result");
    }

    #[test]
    fn writes_success_artifacts_with_eval_result() {
        let dir = tempdir().expect("tempdir");
        let run_dir = dir.path().join("single_eval_success");
        let workdir = dir.path().join("backend_work");
        fs::create_dir_all(&workdir).expect("workdir");
        let candidate = sample_candidate("seed");
        let result = sample_result("relaxed");
        let args = SingleEvalArtifactArgs {
            workdir,
            mode: "ga".into(),
            system: "MgO".into(),
            engine: "GULP".into(),
            backend: EvalBackendKind::Gulp,
            backend_metadata: None,
            procedure_state_digest: None,
            failure_reason: None,
        };

        write_single_eval_artifacts(&run_dir, &args, &candidate, Some(&result))
            .expect("write artifacts");

        assert!(run_dir.join("outputs").join("eval_result.json").exists());
        let status: serde_json::Value = serde_json::from_str(
            &fs::read_to_string(run_dir.join("raw").join("evaluation_status.json"))
                .expect("read status"),
        )
        .expect("parse status");
        assert_eq!(status["status"], "success");
    }
}

fn copy_backend_raw_outputs(
    args: &SingleEvalArtifactArgs,
    raw_dir: &Path,
    structures_dir: &Path,
) -> Result<()> {
    match args.backend {
        EvalBackendKind::Scott => {
            crate::application::scott_run_export::copy_if_exists(
                &args.workdir.join("KLMC.log"),
                &raw_dir.join("KLMC.log"),
            )?;
            crate::application::scott_run_export::copy_if_exists(
                &args.workdir.join("KLMC.err"),
                &raw_dir.join("KLMC.err"),
            )?;
            crate::application::scott_run_export::copy_if_exists(
                &args.workdir.join("run").join("gulp_klmc.gout"),
                &raw_dir.join("gulp_klmc.gout"),
            )?;
            crate::application::scott_run_export::copy_if_exists(
                &args.workdir.join("run").join("gulp_klmc.gin"),
                &raw_dir.join("gulp_klmc.gin"),
            )?;
            crate::application::scott_run_export::copy_if_exists(
                &args.workdir.join("run").join("0").join("gulp_klmc.gout"),
                &raw_dir.join("gulp_klmc.gout"),
            )?;
            crate::application::scott_run_export::copy_if_exists(
                &args.workdir.join("run").join("0").join("gulp_klmc.gin"),
                &raw_dir.join("gulp_klmc.gin"),
            )?;
            crate::application::scott_run_export::copy_structure_outputs(
                &args.workdir.join("run"),
                structures_dir,
            )?;
        }
        EvalBackendKind::Gulp => {
            crate::application::scott_run_export::copy_if_exists(
                &args.workdir.join("candidate.gin"),
                &raw_dir.join("candidate.gin"),
            )?;
            crate::application::scott_run_export::copy_if_exists(
                &args.workdir.join("candidate.got"),
                &raw_dir.join("candidate.got"),
            )?;
            crate::application::scott_run_export::copy_if_exists(
                &args.workdir.join("gulp_klmc.gin"),
                &raw_dir.join("gulp_klmc.gin"),
            )?;
            crate::application::scott_run_export::copy_if_exists(
                &args.workdir.join("gulp_klmc.gout"),
                &raw_dir.join("gulp_klmc.gout"),
            )?;
            crate::application::scott_run_export::copy_if_exists(
                &args.workdir.join("Master.gin"),
                &raw_dir.join("Master.gin"),
            )?;
            crate::application::scott_run_export::copy_if_exists(
                &args.workdir.join("atoms.in"),
                &raw_dir.join("atoms.in"),
            )?;
            crate::application::scott_run_export::copy_if_exists(
                &args.workdir.join("seed.xyz"),
                &raw_dir.join("seed.xyz"),
            )?;
        }
        EvalBackendKind::JanusMace => {
            crate::application::scott_run_export::copy_if_exists(
                &args.workdir.join("candidate.xyz"),
                &raw_dir.join("candidate.xyz"),
            )?;
            crate::application::scott_run_export::copy_if_exists(
                &args.workdir.join("janus_result.json"),
                &raw_dir.join("janus_result.json"),
            )?;
            copy_directory_if_exists(&args.workdir.join("janus_opt"), &raw_dir.join("janus_opt"))?;
        }
    }
    Ok(())
}

fn copy_directory_if_exists(source: &Path, target: &Path) -> Result<()> {
    if !source.exists() {
        return Ok(());
    }
    fs::create_dir_all(target)
        .with_context(|| format!("failed to create directory `{}`", target.display()))?;
    for entry in
        fs::read_dir(source).with_context(|| format!("failed to read `{}`", source.display()))?
    {
        let entry = entry.with_context(|| {
            format!(
                "failed to read directory entry inside `{}`",
                source.display()
            )
        })?;
        let path = entry.path();
        let target_path = target.join(entry.file_name());
        if path.is_dir() {
            copy_directory_if_exists(&path, &target_path)?;
        } else {
            fs::copy(&path, &target_path).with_context(|| {
                format!(
                    "failed to copy `{}` to `{}`",
                    path.display(),
                    target_path.display()
                )
            })?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod mode_tests {
    use super::is_mc_family_mode;

    #[test]
    fn mc_family_mode_aliases_are_recognized() {
        assert!(is_mc_family_mode("mc"));
        assert!(is_mc_family_mode("monte_carlo"));
        assert!(is_mc_family_mode("annealing"));
        assert!(is_mc_family_mode("simulated_annealing"));
        assert!(is_mc_family_mode("energy_lid"));
        assert!(!is_mc_family_mode("bh"));
        assert!(!is_mc_family_mode("ga"));
    }
}
