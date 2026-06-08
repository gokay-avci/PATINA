use anyhow::{Context, Result};
use patina_external::{
    BackendEvaluator, GulpBackend, JanusMaceBackend, JanusMaceConfig, PersistentJanusMaceBackend,
    ScottBackend, ScottSandboxTemplate,
};
use patina_runner::{
    evaluate_requests as evaluate_backend_requests, PersistentWorkerPool, RunConfig,
};
use patina_types::WorkerRequest;
use std::path::{Path, PathBuf};
use std::time::Duration;

#[allow(clippy::too_many_arguments)]
pub fn execute_backend_campaign(
    backend_kind: crate::EvalBackendKind,
    runner_mode: crate::RunnerMode,
    candidate_paths: &[PathBuf],
    generations: usize,
    workdir: &Path,
    workers: Option<usize>,
    keep_dirs: bool,
    timeout_secs: Option<u64>,
    executable: Option<PathBuf>,
    master_gin_template: Option<PathBuf>,
    run_job_template: Option<PathBuf>,
    atoms_in_template: Option<PathBuf>,
    jobs_template: Option<PathBuf>,
    python_bin: Option<PathBuf>,
    janus_adapter_script: Option<PathBuf>,
    janus_arch: String,
    janus_model: String,
    janus_device: String,
    janus_dtype: String,
    janus_mode: crate::DriverJanusMode,
    janus_optimizer: crate::DriverJanusOptimizer,
    janus_fmax: f64,
    janus_steps: usize,
) -> Result<crate::application::report_types::CampaignResult> {
    std::fs::create_dir_all(workdir)
        .with_context(|| format!("failed to create campaign workdir `{}`", workdir.display()))?;

    let worker_count = workers.unwrap_or_else(crate::default_worker_count);
    let timeout = timeout_secs.map(Duration::from_secs);
    let generation_inputs = load_generation_candidates(candidate_paths, generations)?;
    let mut all_responses = Vec::new();
    let mut generation_telemetry = Vec::with_capacity(generations);
    let mut generation_artifacts = Vec::with_capacity(generations);
    let started = std::time::Instant::now();

    match runner_mode {
        crate::RunnerMode::Transient => {
            for (generation_index, requests) in generation_inputs.iter().enumerate() {
                let generation_workdir = workdir.join(format!("generation_{generation_index:04}"));
                let (backend, _metadata) = build_batch_backend(
                    backend_kind,
                    runner_mode,
                    &generation_workdir,
                    keep_dirs,
                    timeout,
                    executable.clone(),
                    master_gin_template.clone(),
                    run_job_template.clone(),
                    atoms_in_template.clone(),
                    jobs_template.clone(),
                    python_bin.clone(),
                    janus_adapter_script.clone(),
                    janus_arch.clone(),
                    janus_model.clone(),
                    janus_device.clone(),
                    janus_dtype.clone(),
                    janus_mode,
                    janus_optimizer,
                    janus_fmax,
                    janus_steps,
                )?;
                let cfg = RunConfig {
                    backend: backend.into(),
                    workdir: generation_workdir,
                    n_workers: worker_count,
                    keep_dirs,
                };
                let generation_started = std::time::Instant::now();
                let responses = evaluate_backend_requests(requests, &cfg)?;
                let elapsed_secs = generation_started.elapsed().as_secs_f64();
                generation_telemetry.push(
                    crate::application::report_types::build_generation_telemetry(
                        generation_index,
                        worker_count,
                        elapsed_secs,
                        &responses,
                    ),
                );
                generation_artifacts.push(
                    crate::application::report_types::build_campaign_generation_artifact(
                        generation_index,
                        elapsed_secs,
                        &responses,
                    ),
                );
                all_responses.push(responses);
            }
        }
        crate::RunnerMode::Persistent => {
            let (backend, backend_metadata) = build_batch_backend(
                backend_kind,
                runner_mode,
                workdir,
                keep_dirs,
                timeout,
                executable,
                master_gin_template,
                run_job_template,
                atoms_in_template,
                jobs_template,
                python_bin,
                janus_adapter_script,
                janus_arch,
                janus_model,
                janus_device,
                janus_dtype,
                janus_mode,
                janus_optimizer,
                janus_fmax,
                janus_steps,
            )?;
            let cfg = RunConfig {
                backend: backend.into(),
                workdir: workdir.to_path_buf(),
                n_workers: worker_count,
                keep_dirs,
            };
            let pool = PersistentWorkerPool::new(cfg)?;
            for (generation_index, requests) in generation_inputs.iter().enumerate() {
                let generation_started = std::time::Instant::now();
                let responses = pool.evaluate_requests(requests)?;
                let elapsed_secs = generation_started.elapsed().as_secs_f64();
                generation_telemetry.push(
                    crate::application::report_types::build_generation_telemetry(
                        generation_index,
                        worker_count,
                        elapsed_secs,
                        &responses,
                    ),
                );
                generation_artifacts.push(
                    crate::application::report_types::build_campaign_generation_artifact(
                        generation_index,
                        elapsed_secs,
                        &responses,
                    ),
                );
                all_responses.push(responses);
            }

            let total_responses = all_responses.iter().flatten().cloned().collect::<Vec<_>>();
            let telemetry = crate::application::report_types::summarize_batch_responses(
                &total_responses,
                started.elapsed().as_secs_f64(),
            );
            return Ok(crate::application::report_types::CampaignResult {
                summary: crate::application::report_types::GenerationRunSummary {
                    backend: backend_kind,
                    runner_mode,
                    workers: worker_count,
                    generations,
                    candidate_count: candidate_paths.len(),
                    keep_dirs,
                    workdir: workdir.to_path_buf(),
                    backend_metadata,
                    telemetry,
                    generation_telemetry,
                },
                generations: generation_artifacts,
                generation_responses: all_responses,
            });
        }
    }

    let (backend, backend_metadata) = build_batch_backend(
        backend_kind,
        runner_mode,
        workdir,
        keep_dirs,
        timeout,
        executable,
        master_gin_template,
        run_job_template,
        atoms_in_template,
        jobs_template,
        python_bin,
        janus_adapter_script,
        janus_arch,
        janus_model,
        janus_device,
        janus_dtype,
        janus_mode,
        janus_optimizer,
        janus_fmax,
        janus_steps,
    )?;
    drop(backend);

    let total_responses = all_responses.iter().flatten().cloned().collect::<Vec<_>>();
    let telemetry = crate::application::report_types::summarize_batch_responses(
        &total_responses,
        started.elapsed().as_secs_f64(),
    );
    Ok(crate::application::report_types::CampaignResult {
        summary: crate::application::report_types::GenerationRunSummary {
            backend: backend_kind,
            runner_mode,
            workers: worker_count,
            generations,
            candidate_count: candidate_paths.len(),
            keep_dirs,
            workdir: workdir.to_path_buf(),
            backend_metadata,
            telemetry,
            generation_telemetry,
        },
        generations: generation_artifacts,
        generation_responses: all_responses,
    })
}

#[allow(clippy::too_many_arguments)]
pub fn run_backend_batch(
    backend_kind: crate::EvalBackendKind,
    runner_mode: crate::RunnerMode,
    candidate_paths: &[PathBuf],
    workdir: &Path,
    workers: Option<usize>,
    keep_dirs: bool,
    timeout_secs: Option<u64>,
    executable: Option<PathBuf>,
    master_gin_template: Option<PathBuf>,
    run_job_template: Option<PathBuf>,
    atoms_in_template: Option<PathBuf>,
    jobs_template: Option<PathBuf>,
    python_bin: Option<PathBuf>,
    janus_adapter_script: Option<PathBuf>,
    janus_arch: String,
    janus_model: String,
    janus_device: String,
    janus_dtype: String,
    janus_mode: crate::DriverJanusMode,
    janus_optimizer: crate::DriverJanusOptimizer,
    janus_fmax: f64,
    janus_steps: usize,
) -> Result<crate::application::report_types::BatchEvaluationSummary> {
    std::fs::create_dir_all(workdir)
        .with_context(|| format!("failed to create batch workdir `{}`", workdir.display()))?;

    let timeout = timeout_secs.map(Duration::from_secs);
    let (backend, backend_metadata) = match (backend_kind, runner_mode) {
        (crate::EvalBackendKind::JanusMace, crate::RunnerMode::Persistent) => {
            let python_bin = crate::absolutize_path(
                &python_bin.unwrap_or_else(patina_external::default_janus_python_bin),
            )?;
            let adapter_script = crate::absolutize_path(
                &janus_adapter_script.unwrap_or_else(patina_external::default_janus_adapter_script),
            )?;
            let config = JanusMaceConfig {
                python_bin: python_bin.clone(),
                adapter_script: adapter_script.clone(),
                arch: janus_arch.clone(),
                model: janus_model.clone(),
                device: janus_device.clone(),
                default_dtype: janus_dtype.clone(),
                mode: janus_mode.into(),
                optimizer: janus_optimizer.into(),
                fmax: janus_fmax,
                steps: janus_steps,
            };
            let metadata = serde_json::json!({
                "python_bin": python_bin,
                "adapter_script": adapter_script,
                "arch": janus_arch,
                "model": janus_model,
                "device": janus_device,
                "dtype": janus_dtype,
                "mode": janus_mode,
                "optimizer": janus_optimizer.as_str(),
                "fmax": janus_fmax,
                "steps": janus_steps,
                "persistent_worker_mode": true
            });
            let backend = PersistentJanusMaceBackend::new(
                config,
                timeout,
                workdir.join("janus_persistent_session"),
                keep_dirs,
            )?;
            (
                Box::new(backend) as Box<dyn BackendEvaluator>,
                Some(metadata),
            )
        }
        _ => build_backend(
            backend_kind,
            timeout,
            executable,
            master_gin_template,
            run_job_template,
            atoms_in_template,
            jobs_template,
            python_bin,
            janus_adapter_script,
            janus_arch,
            janus_model,
            janus_device,
            janus_dtype,
            janus_mode,
            janus_optimizer,
            janus_fmax,
            janus_steps,
        )?,
    };

    let mut requests = Vec::with_capacity(candidate_paths.len());
    for (index, path) in candidate_paths.iter().enumerate() {
        let candidate = crate::read_candidate_json(path)?;
        requests.push(WorkerRequest {
            request_id: format!("candidate_{index:04}"),
            generation: None,
            candidate,
        });
    }

    let cfg = RunConfig {
        backend: backend.into(),
        workdir: workdir.to_path_buf(),
        n_workers: workers.unwrap_or_else(crate::default_worker_count),
        keep_dirs,
    };
    let worker_count = cfg.n_workers;
    let keep_dirs = cfg.keep_dirs;
    let workdir = cfg.workdir.clone();

    let started = std::time::Instant::now();
    let responses = match runner_mode {
        crate::RunnerMode::Transient => evaluate_backend_requests(&requests, &cfg)?,
        crate::RunnerMode::Persistent => {
            let pool = PersistentWorkerPool::new(cfg)?;
            pool.evaluate_requests(&requests)?
        }
    };
    let elapsed_secs = started.elapsed().as_secs_f64();

    let telemetry =
        crate::application::report_types::summarize_batch_responses(&responses, elapsed_secs);

    Ok(crate::application::report_types::BatchEvaluationSummary {
        backend: backend_kind,
        runner_mode,
        workers: worker_count,
        keep_dirs,
        workdir,
        backend_metadata,
        telemetry,
        responses,
    })
}

#[allow(clippy::too_many_arguments)]
pub fn run_backend_generations(
    backend_kind: crate::EvalBackendKind,
    runner_mode: crate::RunnerMode,
    candidate_paths: &[PathBuf],
    generations: usize,
    workdir: &Path,
    workers: Option<usize>,
    keep_dirs: bool,
    timeout_secs: Option<u64>,
    executable: Option<PathBuf>,
    master_gin_template: Option<PathBuf>,
    run_job_template: Option<PathBuf>,
    atoms_in_template: Option<PathBuf>,
    jobs_template: Option<PathBuf>,
    python_bin: Option<PathBuf>,
    janus_adapter_script: Option<PathBuf>,
    janus_arch: String,
    janus_model: String,
    janus_device: String,
    janus_dtype: String,
    janus_mode: crate::DriverJanusMode,
    janus_optimizer: crate::DriverJanusOptimizer,
    janus_fmax: f64,
    janus_steps: usize,
) -> Result<crate::application::report_types::GenerationRunSummary> {
    std::fs::create_dir_all(workdir).with_context(|| {
        format!(
            "failed to create generation workdir `{}`",
            workdir.display()
        )
    })?;

    let worker_count = workers.unwrap_or_else(crate::default_worker_count);
    let timeout = timeout_secs.map(Duration::from_secs);
    let generation_inputs = load_generation_candidates(candidate_paths, generations)?;
    let started = std::time::Instant::now();
    let mut total_responses = Vec::new();
    let mut generation_telemetry = Vec::with_capacity(generations);

    match runner_mode {
        crate::RunnerMode::Transient => {
            for (generation_index, requests) in generation_inputs.iter().enumerate() {
                let generation_workdir = workdir.join(format!("generation_{generation_index:04}"));
                let (backend, _metadata) = build_batch_backend(
                    backend_kind,
                    runner_mode,
                    &generation_workdir,
                    keep_dirs,
                    timeout,
                    executable.clone(),
                    master_gin_template.clone(),
                    run_job_template.clone(),
                    atoms_in_template.clone(),
                    jobs_template.clone(),
                    python_bin.clone(),
                    janus_adapter_script.clone(),
                    janus_arch.clone(),
                    janus_model.clone(),
                    janus_device.clone(),
                    janus_dtype.clone(),
                    janus_mode,
                    janus_optimizer,
                    janus_fmax,
                    janus_steps,
                )?;
                let cfg = RunConfig {
                    backend: backend.into(),
                    workdir: generation_workdir,
                    n_workers: worker_count,
                    keep_dirs,
                };
                let generation_started = std::time::Instant::now();
                let responses = evaluate_backend_requests(requests, &cfg)?;
                generation_telemetry.push(
                    crate::application::report_types::build_generation_telemetry(
                        generation_index,
                        worker_count,
                        generation_started.elapsed().as_secs_f64(),
                        &responses,
                    ),
                );
                total_responses.extend(responses);
            }
        }
        crate::RunnerMode::Persistent => {
            let (backend, backend_metadata) = build_batch_backend(
                backend_kind,
                runner_mode,
                workdir,
                keep_dirs,
                timeout,
                executable,
                master_gin_template,
                run_job_template,
                atoms_in_template,
                jobs_template,
                python_bin,
                janus_adapter_script,
                janus_arch,
                janus_model,
                janus_device,
                janus_dtype,
                janus_mode,
                janus_optimizer,
                janus_fmax,
                janus_steps,
            )?;
            let cfg = RunConfig {
                backend: backend.into(),
                workdir: workdir.to_path_buf(),
                n_workers: worker_count,
                keep_dirs,
            };
            let pool = PersistentWorkerPool::new(cfg)?;
            for (generation_index, requests) in generation_inputs.iter().enumerate() {
                let generation_started = std::time::Instant::now();
                let responses = pool.evaluate_requests(requests)?;
                generation_telemetry.push(
                    crate::application::report_types::build_generation_telemetry(
                        generation_index,
                        worker_count,
                        generation_started.elapsed().as_secs_f64(),
                        &responses,
                    ),
                );
                total_responses.extend(responses);
            }

            let telemetry = crate::application::report_types::summarize_batch_responses(
                &total_responses,
                started.elapsed().as_secs_f64(),
            );
            return Ok(crate::application::report_types::GenerationRunSummary {
                backend: backend_kind,
                runner_mode,
                workers: worker_count,
                generations,
                candidate_count: candidate_paths.len(),
                keep_dirs,
                workdir: workdir.to_path_buf(),
                backend_metadata,
                telemetry,
                generation_telemetry,
            });
        }
    }

    let (backend, backend_metadata) = build_batch_backend(
        backend_kind,
        runner_mode,
        workdir,
        keep_dirs,
        timeout,
        executable,
        master_gin_template,
        run_job_template,
        atoms_in_template,
        jobs_template,
        python_bin,
        janus_adapter_script,
        janus_arch,
        janus_model,
        janus_device,
        janus_dtype,
        janus_mode,
        janus_optimizer,
        janus_fmax,
        janus_steps,
    )?;
    drop(backend);

    let telemetry = crate::application::report_types::summarize_batch_responses(
        &total_responses,
        started.elapsed().as_secs_f64(),
    );
    Ok(crate::application::report_types::GenerationRunSummary {
        backend: backend_kind,
        runner_mode,
        workers: worker_count,
        generations,
        candidate_count: candidate_paths.len(),
        keep_dirs,
        workdir: workdir.to_path_buf(),
        backend_metadata,
        telemetry,
        generation_telemetry,
    })
}

#[allow(clippy::too_many_arguments)]
pub fn build_batch_backend(
    backend_kind: crate::EvalBackendKind,
    runner_mode: crate::RunnerMode,
    workdir: &Path,
    keep_dirs: bool,
    timeout: Option<Duration>,
    executable: Option<PathBuf>,
    master_gin_template: Option<PathBuf>,
    run_job_template: Option<PathBuf>,
    atoms_in_template: Option<PathBuf>,
    jobs_template: Option<PathBuf>,
    python_bin: Option<PathBuf>,
    janus_adapter_script: Option<PathBuf>,
    janus_arch: String,
    janus_model: String,
    janus_device: String,
    janus_dtype: String,
    janus_mode: crate::DriverJanusMode,
    janus_optimizer: crate::DriverJanusOptimizer,
    janus_fmax: f64,
    janus_steps: usize,
) -> Result<(Box<dyn BackendEvaluator>, Option<serde_json::Value>)> {
    match (backend_kind, runner_mode) {
        (crate::EvalBackendKind::JanusMace, crate::RunnerMode::Persistent) => {
            let python_bin = crate::absolutize_path(
                &python_bin.unwrap_or_else(patina_external::default_janus_python_bin),
            )?;
            let adapter_script = crate::absolutize_path(
                &janus_adapter_script.unwrap_or_else(patina_external::default_janus_adapter_script),
            )?;
            let config = JanusMaceConfig {
                python_bin: python_bin.clone(),
                adapter_script: adapter_script.clone(),
                arch: janus_arch.clone(),
                model: janus_model.clone(),
                device: janus_device.clone(),
                default_dtype: janus_dtype.clone(),
                mode: janus_mode.into(),
                optimizer: janus_optimizer.into(),
                fmax: janus_fmax,
                steps: janus_steps,
            };
            let metadata = serde_json::json!({
                "python_bin": python_bin,
                "adapter_script": adapter_script,
                "arch": janus_arch,
                "model": janus_model,
                "device": janus_device,
                "dtype": janus_dtype,
                "mode": janus_mode,
                "optimizer": janus_optimizer.as_str(),
                "fmax": janus_fmax,
                "steps": janus_steps,
                "persistent_worker_mode": true
            });
            let backend = PersistentJanusMaceBackend::new(
                config,
                timeout,
                workdir.join("janus_persistent_session"),
                keep_dirs,
            )?;
            Ok((
                Box::new(backend) as Box<dyn BackendEvaluator>,
                Some(metadata),
            ))
        }
        _ => build_backend(
            backend_kind,
            timeout,
            executable,
            master_gin_template,
            run_job_template,
            atoms_in_template,
            jobs_template,
            python_bin,
            janus_adapter_script,
            janus_arch,
            janus_model,
            janus_device,
            janus_dtype,
            janus_mode,
            janus_optimizer,
            janus_fmax,
            janus_steps,
        ),
    }
}

#[allow(clippy::too_many_arguments)]
pub fn build_backend(
    backend_kind: crate::EvalBackendKind,
    timeout: Option<Duration>,
    executable: Option<PathBuf>,
    master_gin_template: Option<PathBuf>,
    run_job_template: Option<PathBuf>,
    atoms_in_template: Option<PathBuf>,
    jobs_template: Option<PathBuf>,
    python_bin: Option<PathBuf>,
    janus_adapter_script: Option<PathBuf>,
    janus_arch: String,
    janus_model: String,
    janus_device: String,
    janus_dtype: String,
    janus_mode: crate::DriverJanusMode,
    janus_optimizer: crate::DriverJanusOptimizer,
    janus_fmax: f64,
    janus_steps: usize,
) -> Result<(Box<dyn BackendEvaluator>, Option<serde_json::Value>)> {
    match backend_kind {
        crate::EvalBackendKind::Scott => {
            let executable =
                crate::absolutize_path(&crate::required_arg(executable, "executable", "scott")?)?;
            let master_gin_template = crate::absolutize_path(&crate::required_arg(
                master_gin_template,
                "master-gin-template",
                "scott",
            )?)?;
            let run_job_template = crate::absolutize_path(&crate::required_arg(
                run_job_template,
                "run-job-template",
                "scott",
            )?)?;
            let atoms_in_template = crate::absolutize_path(&crate::required_arg(
                atoms_in_template,
                "atoms-in-template",
                "scott",
            )?)?;
            let jobs_template = jobs_template
                .as_ref()
                .map(|path| crate::absolutize_path(path))
                .transpose()?;
            let backend = ScottBackend::new(
                &master_gin_template,
                &executable,
                ScottSandboxTemplate::new(run_job_template, atoms_in_template, jobs_template),
                timeout,
            )?;
            Ok((Box::new(backend), None))
        }
        crate::EvalBackendKind::Gulp => {
            let executable =
                crate::absolutize_path(&crate::required_arg(executable, "executable", "gulp")?)?;
            let master_gin_template = crate::absolutize_path(&crate::required_arg(
                master_gin_template,
                "master-gin-template",
                "gulp",
            )?)?;
            let metadata = serde_json::json!({
                "executable": executable,
                "master_gin_template": master_gin_template,
            });
            Ok((
                Box::new(GulpBackend::new(
                    &master_gin_template,
                    &executable,
                    timeout,
                )?),
                Some(metadata),
            ))
        }
        crate::EvalBackendKind::JanusMace => {
            let python_bin = crate::absolutize_path(
                &python_bin.unwrap_or_else(patina_external::default_janus_python_bin),
            )?;
            let adapter_script = crate::absolutize_path(
                &janus_adapter_script.unwrap_or_else(patina_external::default_janus_adapter_script),
            )?;
            let config = JanusMaceConfig {
                python_bin: python_bin.clone(),
                adapter_script: adapter_script.clone(),
                arch: janus_arch.clone(),
                model: janus_model.clone(),
                device: janus_device.clone(),
                default_dtype: janus_dtype.clone(),
                mode: janus_mode.into(),
                optimizer: janus_optimizer.into(),
                fmax: janus_fmax,
                steps: janus_steps,
            };
            let metadata = serde_json::json!({
                "python_bin": python_bin,
                "adapter_script": adapter_script,
                "arch": janus_arch,
                "model": janus_model,
                "device": janus_device,
                "dtype": janus_dtype,
                "mode": janus_mode,
                "optimizer": janus_optimizer.as_str(),
                "fmax": janus_fmax,
                "steps": janus_steps
            });
            Ok((
                Box::new(JanusMaceBackend::new(config, timeout)),
                Some(metadata),
            ))
        }
    }
}

pub fn load_generation_candidates(
    candidate_paths: &[PathBuf],
    generations: usize,
) -> Result<Vec<Vec<WorkerRequest>>> {
    let mut base_candidates = Vec::with_capacity(candidate_paths.len());
    for path in candidate_paths {
        base_candidates.push(crate::read_candidate_json(path)?);
    }

    let mut all_generations = Vec::with_capacity(generations);
    for generation_index in 0..generations {
        let requests = base_candidates
            .iter()
            .enumerate()
            .map(|(candidate_index, candidate)| {
                let mut candidate = candidate.clone();
                candidate.label =
                    format!("gen_{generation_index:04}_candidate_{candidate_index:04}");
                WorkerRequest {
                    request_id: candidate.label.clone(),
                    generation: Some(generation_index),
                    candidate,
                }
            })
            .collect();
        all_generations.push(requests);
    }
    Ok(all_generations)
}
