use crate::fingerprints::{
    fingerprint_candidate, fingerprint_candidate_with_symmetry, group_by_jaccard, SignatureRecord,
};
use crate::generators::{
    generate_candidates, ChemicalEdgePolicy, DegreeBounds, GenerationConstraints, GenerationRequest,
};
use crate::io::figures::write_preview_bundle;
use crate::io::jsonl::{
    read_candidates_jsonl, read_signatures_jsonl, write_candidates_jsonl, write_signatures_jsonl,
};
use crate::io::provenance::{append_checkpoint, artifact, write_manifest};
use crate::io::xyz::{
    sanitize_path_component, write_candidate_xyz, write_classified_xyz_directory,
    write_xyz_directory,
};
use crate::ports::SyvaPointSymmetryBackend;
use crate::{estimate_bond_length_from_formula, BondLengthEstimate, BondLengthMode};
use crate::{CheckpointChecklist, CheckpointStatus, ElementSymbol};
use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Debug, Parser)]
#[command(name = "patina-topogen")]
#[command(about = "Generate, fingerprint, group, and inspect PATINA topology motifs")]
pub struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Debug, Subcommand)]
enum Command {
    Generate(GenerateArgs),
    Fingerprint(FingerprintArgs),
    Group(GroupArgs),
    Inspect(InspectArgs),
    ExportXyz(ExportXyzArgs),
    Preview(PreviewArgs),
    Symmetrize(SymmetrizeArgs),
}

#[derive(Debug, Parser)]
struct GenerateArgs {
    #[arg(long)]
    formula: String,
    #[arg(long, default_value_t = 1)]
    n_min: usize,
    #[arg(long, default_value_t = 1)]
    n_max: usize,
    #[arg(long)]
    bond_length: Option<f64>,
    #[arg(long, default_value = "ionic")]
    bond_length_mode: String,
    #[arg(long, default_value_t = 0.0)]
    bond_radius_const: f64,
    #[arg(long, default_value_t = 1.5)]
    bond_length_fallback: f64,
    #[arg(long, value_delimiter = ',', default_value = "ring,barrel,wire")]
    generators: Vec<String>,
    #[arg(long, default_value_t = 1000)]
    max_candidates: usize,
    #[arg(long)]
    seed: Option<u64>,
    #[arg(long, value_delimiter = ',')]
    degree: Vec<String>,
    #[arg(long, default_value = "prefer-hetero")]
    edge_policy: String,
    #[arg(long, default_value = "0.75:1.25")]
    bond_window: String,
    #[arg(long, default_value_t = 0.60)]
    min_nonbonded_factor: f64,
    #[arg(long)]
    target_extra_edges: Option<usize>,
    #[arg(long, default_value_t = 32)]
    mc_proposals: usize,
    #[arg(long, default_value_t = 8)]
    mc_max_accept: usize,
    #[arg(long, default_value_t = 0.20)]
    mc_min_jaccard_distance: f64,
    #[arg(long)]
    out: PathBuf,
}

#[derive(Debug, Parser)]
struct FingerprintArgs {
    #[arg(long)]
    input: PathBuf,
    #[arg(long)]
    out: PathBuf,
    #[arg(long, default_value = "none")]
    symmetry_backend: String,
    #[arg(long, default_value_t = 0.001)]
    symmetry_tolerance: f64,
}

#[derive(Debug, Parser)]
struct GroupArgs {
    #[arg(long)]
    input: PathBuf,
    #[arg(long, default_value = "jaccard")]
    method: String,
    #[arg(long, default_value_t = 0.75)]
    threshold: f64,
    #[arg(long)]
    out: PathBuf,
}

#[derive(Debug, Parser)]
struct InspectArgs {
    #[arg(long)]
    input: PathBuf,
    #[arg(long, alias = "top", default_value_t = 20)]
    limit: usize,
    #[arg(long, default_value = "n_atoms")]
    sort_by: String,
}

#[derive(Debug, Parser)]
struct ExportXyzArgs {
    #[arg(long)]
    input: PathBuf,
    #[arg(long)]
    signatures: Option<PathBuf>,
    #[arg(long, default_value = "flat")]
    classify_by: String,
    #[arg(long)]
    out: PathBuf,
}

#[derive(Debug, Parser)]
struct PreviewArgs {
    #[arg(long)]
    candidates: PathBuf,
    #[arg(long)]
    signatures: PathBuf,
    #[arg(long, default_value_t = 12)]
    limit: usize,
    #[arg(long, default_value_t = 0.75)]
    jaccard_threshold: f64,
    #[arg(long)]
    out: PathBuf,
}

#[derive(Debug, Parser)]
struct SymmetrizeArgs {
    #[arg(long)]
    input: PathBuf,
    #[arg(long)]
    out: PathBuf,
    #[arg(long)]
    report: Option<PathBuf>,
    #[arg(long, default_value_t = 0.001)]
    symmetry_tolerance: f64,
    #[arg(long, default_value_t = false)]
    strict: bool,
}

pub fn run() -> Result<()> {
    match Cli::parse().command {
        Command::Generate(args) => generate(args),
        Command::Fingerprint(args) => fingerprint(args),
        Command::Group(args) => group(args),
        Command::Inspect(args) => inspect(args),
        Command::ExportXyz(args) => export_xyz(args),
        Command::Preview(args) => preview(args),
        Command::Symmetrize(args) => symmetrize(args),
    }
}

fn generate(args: GenerateArgs) -> Result<()> {
    fs::create_dir_all(&args.out).with_context(|| format!("create `{}`", args.out.display()))?;
    let bond_length = resolve_bond_length(&args)?;
    let constraints = generation_constraints(&args, bond_length.bond_length)?;
    let request = GenerationRequest {
        formula: args.formula.clone(),
        n_min: args.n_min,
        n_max: args.n_max,
        bond_length: bond_length.bond_length,
        bond_length_source: Some(format!(
            "{}; mode={:?}; pair={:?}; {}",
            bond_length.source, bond_length.mode, bond_length.pair, bond_length.message
        )),
        generator_names: args.generators.clone(),
        max_candidates: args.max_candidates,
        seed: args.seed,
        constraints,
    };
    let mut candidates = generate_candidates(&request)?;
    if let Some(source) = &request.bond_length_source {
        for candidate in &mut candidates {
            candidate.parameters.insert(
                "bond_length_source".to_string(),
                serde_json::Value::String(source.clone()),
            );
        }
    }
    let generation_summary = generation_summary(&candidates);
    let path = args.out.join("candidates.jsonl");
    write_candidates_jsonl(&path, &candidates)?;
    let logs = args.out.join("logs").join("checkpoints.jsonl");
    let manifest = args.out.join("manifest.json");
    let checklist = CheckpointChecklist::topology_generation()
        .mark(
            "formula_parsed",
            CheckpointStatus::Passed,
            format!(
                "formula={} n={}..{}",
                request.formula, request.n_min, request.n_max
            ),
        )
        .mark(
            "generators_selected",
            CheckpointStatus::Passed,
            request.generator_names.join(","),
        )
        .mark(
            "graph_validated",
            CheckpointStatus::Passed,
            format!(
                "{} candidates generated; validation_passed={} validation_failed={}",
                candidates.len(),
                generation_summary.validation_passed,
                generation_summary.validation_failed
            ),
        )
        .mark(
            "jsonl_written",
            CheckpointStatus::Passed,
            path.display().to_string(),
        )
        .mark(
            "no_energy_dependency",
            CheckpointStatus::Passed,
            "generation path does not call evaluator backends",
        );
    let artifacts = vec![artifact(
        &path,
        "jsonl",
        "motif_candidates",
        Some(candidates.len()),
    )];
    write_manifest(
        &manifest,
        "patina-topology-library",
        "generate",
        generation_inputs(&request, &generation_summary),
        artifacts.clone(),
        checklist,
    )?;
    append_checkpoint(
        &logs,
        "generate",
        CheckpointStatus::Passed,
        format!(
            "generated {} topology candidates; validation_passed={} validation_failed={} rejected_degree={} rejected_chemical={} mc_duplicate_wl={} mc_similarity={}",
            candidates.len(),
            generation_summary.validation_passed,
            generation_summary.validation_failed,
            generation_summary.rejected_degree,
            generation_summary.rejected_chemical_policy,
            generation_summary.mc_rejected_duplicate_wl,
            generation_summary.mc_rejected_similarity
        ),
        artifacts,
    )?;
    println!(
        "wrote {} candidates to {}",
        candidates.len(),
        path.display()
    );
    Ok(())
}

fn resolve_bond_length(args: &GenerateArgs) -> Result<BondLengthEstimate> {
    if let Some(bond_length) = args.bond_length {
        return Ok(BondLengthEstimate {
            bond_length,
            mode: BondLengthMode::parse(&args.bond_length_mode)?,
            source: "cli --bond-length".to_string(),
            pair: None,
            message: "explicit CLI override".to_string(),
        });
    }
    let mode = BondLengthMode::parse(&args.bond_length_mode)?;
    Ok(estimate_bond_length_from_formula(
        &args.formula,
        mode,
        args.bond_radius_const,
        args.bond_length_fallback,
    )?)
}

fn generation_constraints(args: &GenerateArgs, bond_length: f64) -> Result<GenerationConstraints> {
    let mut constraints = GenerationConstraints::for_bond_length(bond_length);
    let (bond_min_factor, bond_max_factor) = parse_pair_f64(&args.bond_window, "bond-window")?;
    constraints.validation.bond_min_factor = bond_min_factor;
    constraints.validation.bond_max_factor = bond_max_factor;
    constraints.validation.min_nonbonded_factor = args.min_nonbonded_factor;
    constraints.degree_bounds = parse_degree_bounds(&args.degree)?;
    constraints.edge_policy = parse_edge_policy(&args.edge_policy)?;
    constraints.target_extra_edges = args.target_extra_edges;
    constraints.mc_proposals = args.mc_proposals;
    constraints.mc_max_accept = args.mc_max_accept;
    constraints.mc_min_jaccard_distance = args.mc_min_jaccard_distance;
    Ok(constraints)
}

fn parse_degree_bounds(values: &[String]) -> Result<BTreeMap<ElementSymbol, DegreeBounds>> {
    let mut out = BTreeMap::new();
    for value in values {
        let parts = value.split(':').collect::<Vec<_>>();
        if parts.len() != 3 {
            anyhow::bail!("invalid --degree `{value}`; expected Element:min:max, e.g. Ti:3:6");
        }
        let min = parts[1]
            .parse::<usize>()
            .with_context(|| format!("invalid min degree in `{value}`"))?;
        let max = parts[2]
            .parse::<usize>()
            .with_context(|| format!("invalid max degree in `{value}`"))?;
        if min > max {
            anyhow::bail!("invalid --degree `{value}`; min must be <= max");
        }
        out.insert(
            ElementSymbol(parts[0].to_string()),
            DegreeBounds { min, max },
        );
    }
    Ok(out)
}

fn parse_edge_policy(value: &str) -> Result<ChemicalEdgePolicy> {
    match value.replace('_', "-").to_ascii_lowercase().as_str() {
        "any" => Ok(ChemicalEdgePolicy::Any),
        "prefer-hetero" | "preferhetero" => Ok(ChemicalEdgePolicy::PreferHetero),
        "require-hetero" | "requirehetero" => Ok(ChemicalEdgePolicy::RequireHetero),
        other => anyhow::bail!(
            "invalid --edge-policy `{other}`; expected any, prefer-hetero, or require-hetero"
        ),
    }
}

fn parse_pair_f64(value: &str, label: &str) -> Result<(f64, f64)> {
    let parts = value.split(':').collect::<Vec<_>>();
    if parts.len() != 2 {
        anyhow::bail!("invalid --{label} `{value}`; expected min:max");
    }
    let left = parts[0]
        .parse::<f64>()
        .with_context(|| format!("invalid first value in --{label} `{value}`"))?;
    let right = parts[1]
        .parse::<f64>()
        .with_context(|| format!("invalid second value in --{label} `{value}`"))?;
    if left > right {
        anyhow::bail!("invalid --{label} `{value}`; min must be <= max");
    }
    Ok((left, right))
}

fn fingerprint(args: FingerprintArgs) -> Result<()> {
    let candidates = read_candidates_jsonl(&args.input)?;
    if let Some(parent) = args.out.parent() {
        fs::create_dir_all(parent)?;
    }
    let syva_backend = match args.symmetry_backend.as_str() {
        "none" => None,
        "syva" => Some(SyvaPointSymmetryBackend::new(args.symmetry_tolerance)),
        other => anyhow::bail!("unsupported --symmetry-backend `{other}`; expected none or syva"),
    };
    let symmetry_backend = syva_backend
        .as_ref()
        .map(|backend| backend as &dyn crate::ports::PointSymmetryBackend);
    let records = candidates
        .iter()
        .map(|candidate| SignatureRecord {
            candidate_id: candidate.id.0.clone(),
            generator: candidate.generator.name.clone(),
            signature: fingerprint_candidate_with_symmetry(candidate, symmetry_backend),
        })
        .collect::<Vec<_>>();
    write_signatures_jsonl(&args.out, &records)?;
    let run_dir = args.out.parent().unwrap_or_else(|| Path::new("."));
    let artifacts = vec![artifact(
        &args.out,
        "jsonl",
        "topology_signatures",
        Some(records.len()),
    )];
    let checklist = CheckpointChecklist::fingerprinting()
        .mark(
            "input_loaded",
            CheckpointStatus::Passed,
            format!("{} candidates", candidates.len()),
        )
        .mark(
            "graph_signature",
            CheckpointStatus::Passed,
            "graph/ring/coordination",
        )
        .mark(
            "hash_cascade",
            CheckpointStatus::Passed,
            "fast+WL+geometry+full",
        )
        .mark(
            "symmetry_backend",
            CheckpointStatus::Passed,
            format!(
                "backend={} tolerance={}",
                args.symmetry_backend, args.symmetry_tolerance
            ),
        )
        .mark(
            "jsonl_written",
            CheckpointStatus::Passed,
            args.out.display().to_string(),
        )
        .mark(
            "uniqueness_guardrail",
            CheckpointStatus::Passed,
            "canonical_graph_hash remains null unless backend is supplied",
        );
    write_manifest(
        run_dir.join("fingerprint_manifest.json"),
        "patina-topology-library",
        "fingerprint",
        BTreeMap::from([
            ("input".to_string(), args.input.display().to_string()),
            (
                "symmetry_backend".to_string(),
                args.symmetry_backend.to_string(),
            ),
            (
                "symmetry_tolerance".to_string(),
                args.symmetry_tolerance.to_string(),
            ),
        ]),
        artifacts.clone(),
        checklist,
    )?;
    append_checkpoint(
        run_dir.join("logs").join("checkpoints.jsonl"),
        "fingerprint",
        CheckpointStatus::Passed,
        format!("fingerprinted {} candidates", records.len()),
        artifacts,
    )?;
    println!(
        "wrote {} signatures to {}",
        records.len(),
        args.out.display()
    );
    Ok(())
}

fn group(args: GroupArgs) -> Result<()> {
    if args.method != "jaccard" {
        anyhow::bail!("only --method jaccard is implemented in phase 1");
    }
    let records = read_signatures_jsonl(&args.input)?;
    let output = group_by_jaccard(&records, args.threshold);
    if let Some(parent) = args.out.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(&args.out, serde_json::to_string_pretty(&output)?)?;
    let run_dir = args.out.parent().unwrap_or_else(|| Path::new("."));
    let artifacts = vec![artifact(
        &args.out,
        "json",
        "jaccard_motif_groups",
        Some(output.groups.len()),
    )];
    let checklist = CheckpointChecklist::grouping()
        .mark(
            "input_loaded",
            CheckpointStatus::Passed,
            format!("{} signatures", records.len()),
        )
        .mark(
            "similarity_defined",
            CheckpointStatus::Passed,
            format!("method=jaccard threshold={}", args.threshold),
        )
        .mark(
            "components_written",
            CheckpointStatus::Passed,
            args.out.display().to_string(),
        );
    write_manifest(
        run_dir.join("group_manifest.json"),
        "patina-topology-library",
        "group",
        BTreeMap::from([
            ("input".to_string(), args.input.display().to_string()),
            ("threshold".to_string(), args.threshold.to_string()),
        ]),
        artifacts.clone(),
        checklist,
    )?;
    append_checkpoint(
        run_dir.join("logs").join("checkpoints.jsonl"),
        "group",
        CheckpointStatus::Passed,
        format!(
            "grouped {} signatures into {} components",
            records.len(),
            output.groups.len()
        ),
        artifacts,
    )?;
    println!(
        "wrote {} groups to {}",
        output.groups.len(),
        args.out.display()
    );
    Ok(())
}

fn inspect(args: InspectArgs) -> Result<()> {
    let mut records = read_signatures_jsonl(&args.input)?;
    match args.sort_by.as_str() {
        "cycle_rank" => records.sort_by_key(|record| record.signature.graph_basic.cycle_rank),
        "edges" | "n_edges" => records.sort_by_key(|record| record.signature.graph_basic.n_edges),
        "rings" => records.sort_by_key(|record| record.signature.rings.cycle_basis_lengths.len()),
        "automorphism_order" => records.sort_by_key(|record| {
            record
                .signature
                .hashes
                .canonical_graph_hash
                .clone()
                .unwrap_or_default()
        }),
        _ => records.sort_by_key(|record| record.signature.n_atoms),
    }
    records.reverse();
    println!(
        "{:<18} {:<8} {:<8} {:<8} {:<10} fast_hash",
        "candidate", "atoms", "edges", "cycles", "morph"
    );
    for record in records.iter().take(args.limit) {
        println!(
            "{:<18} {:<8} {:<8} {:<8} {:<10} {}",
            record.candidate_id,
            record.signature.n_atoms,
            record.signature.graph_basic.n_edges,
            record.signature.graph_basic.cycle_rank,
            record.signature.geometry.morphology_label,
            record.signature.hashes.fast_hash
        );
    }
    Ok(())
}

fn export_xyz(args: ExportXyzArgs) -> Result<()> {
    let mut candidates = read_candidates_jsonl(&args.input)?;
    let signatures = if let Some(path) = &args.signatures {
        read_signatures_jsonl(path)?
            .into_iter()
            .map(|record| (record.candidate_id.clone(), record.signature))
            .collect::<BTreeMap<_, _>>()
    } else {
        BTreeMap::new()
    };
    for candidate in &mut candidates {
        candidate.topology_signature = signatures
            .get(&candidate.id.0)
            .cloned()
            .or_else(|| Some(fingerprint_candidate(candidate)));
    }
    if args.classify_by == "flat" {
        write_xyz_directory(&args.out, &candidates)?;
    } else {
        let classified = candidates
            .iter()
            .map(|candidate| {
                let class_name = xyz_class_name(candidate, &args.classify_by);
                (candidate, class_name)
            })
            .collect::<Vec<_>>();
        write_classified_xyz_directory(&args.out, classified)?;
    }
    let run_dir = args.out.parent().unwrap_or_else(|| Path::new("."));
    let artifacts = vec![artifact(
        &args.out,
        "xyz_directory",
        format!("xyz_export_classified_by_{}", args.classify_by),
        Some(candidates.len()),
    )];
    append_checkpoint(
        run_dir.join("logs").join("checkpoints.jsonl"),
        "export_xyz",
        CheckpointStatus::Passed,
        format!(
            "exported {} xyz files classified by {}",
            candidates.len(),
            args.classify_by
        ),
        artifacts,
    )?;
    println!(
        "wrote {} xyz files to {}",
        candidates.len(),
        args.out.display()
    );
    Ok(())
}

fn xyz_class_name(candidate: &crate::MotifCandidate, classify_by: &str) -> String {
    let signature = candidate
        .topology_signature
        .as_ref()
        .expect("export_xyz attaches signatures before classification");
    match classify_by {
        "generator" => candidate.generator.name.clone(),
        "morphology" | "topology" => signature.geometry.morphology_label.clone(),
        "formula" => candidate.composition.total_formula(),
        "fast_hash" => signature.hashes.fast_hash.clone(),
        "wl_hash" => signature.hashes.wl_hash.clone(),
        "full_hash" => signature.hashes.full_signature_hash.clone(),
        other => format!("unknown_classification_{other}"),
    }
}

fn preview(args: PreviewArgs) -> Result<()> {
    let candidates = read_candidates_jsonl(&args.candidates)?;
    let signatures = read_signatures_jsonl(&args.signatures)?;
    let artifacts = write_preview_bundle(
        &args.out,
        &candidates,
        &signatures,
        args.limit,
        args.jaccard_threshold,
    )?;
    let artifact_records = vec![
        artifact(&artifacts.motif_gallery_svg, "svg", "motif_gallery", None),
        artifact(
            &artifacts.signature_summary_svg,
            "svg",
            "signature_summary",
            None,
        ),
        artifact(
            &artifacts.symmetry_summary_svg,
            "svg",
            "symmetry_summary",
            None,
        ),
        artifact(
            &artifacts.validation_summary_svg,
            "svg",
            "validation_summary",
            None,
        ),
        artifact(
            &artifacts.generator_morphology_matrix_svg,
            "svg",
            "generator_morphology_matrix",
            None,
        ),
        artifact(
            &artifacts.topology_metrics_svg,
            "svg",
            "topology_metrics",
            None,
        ),
        artifact(&artifacts.jaccard_graph_dot, "dot", "jaccard_graph", None),
        artifact(
            &artifacts.candidate_graphs_dot,
            "dot",
            "candidate_graphs",
            None,
        ),
        artifact(&artifacts.atlas_html, "html", "topology_atlas", None),
    ];
    let checklist = CheckpointChecklist::preview()
        .mark(
            "candidate_projection",
            CheckpointStatus::Passed,
            artifacts.motif_gallery_svg.display().to_string(),
        )
        .mark(
            "signature_summary",
            CheckpointStatus::Passed,
            artifacts.signature_summary_svg.display().to_string(),
        )
        .mark(
            "symmetry_summary",
            CheckpointStatus::Passed,
            artifacts.symmetry_summary_svg.display().to_string(),
        )
        .mark(
            "validation_summary",
            CheckpointStatus::Passed,
            artifacts.validation_summary_svg.display().to_string(),
        )
        .mark(
            "generator_morphology_matrix",
            CheckpointStatus::Passed,
            artifacts
                .generator_morphology_matrix_svg
                .display()
                .to_string(),
        )
        .mark(
            "topology_metrics",
            CheckpointStatus::Passed,
            artifacts.topology_metrics_svg.display().to_string(),
        )
        .mark(
            "similarity_graph",
            CheckpointStatus::Passed,
            artifacts.jaccard_graph_dot.display().to_string(),
        )
        .mark(
            "atlas_written",
            CheckpointStatus::Passed,
            artifacts.atlas_html.display().to_string(),
        );
    write_manifest(
        args.out.join("preview_manifest.json"),
        "patina-topology-library",
        "preview",
        BTreeMap::from([
            (
                "candidates".to_string(),
                args.candidates.display().to_string(),
            ),
            (
                "signatures".to_string(),
                args.signatures.display().to_string(),
            ),
            (
                "jaccard_threshold".to_string(),
                args.jaccard_threshold.to_string(),
            ),
        ]),
        artifact_records.clone(),
        checklist,
    )?;
    append_checkpoint(
        args.out.join("logs").join("checkpoints.jsonl"),
        "preview",
        CheckpointStatus::Passed,
        "wrote SVG/DOT/HTML topology atlas artifacts",
        artifact_records,
    )?;
    println!(
        "wrote preview artifacts to {}; open {}",
        args.out.display(),
        artifacts.atlas_html.display()
    );
    Ok(())
}

fn symmetrize(args: SymmetrizeArgs) -> Result<()> {
    let candidates = read_candidates_jsonl(&args.input)?;
    fs::create_dir_all(&args.out)?;
    let report_path = args
        .report
        .clone()
        .unwrap_or_else(|| args.out.join("symmetrization.jsonl"));
    if let Some(parent) = report_path.parent() {
        fs::create_dir_all(parent)?;
    }
    let backend = SyvaPointSymmetryBackend::new(args.symmetry_tolerance);
    let mut records = Vec::with_capacity(candidates.len());
    for candidate in &candidates {
        let mut record = backend.symmetrize_candidate(candidate, args.strict);
        if let Some(coordinates) = &record.coordinates {
            let point_group = record
                .point_group
                .as_deref()
                .map(sanitize_path_component)
                .unwrap_or_else(|| "unclassified".to_string());
            let class_dir = args.out.join(point_group);
            fs::create_dir_all(&class_dir)?;
            let xyz_path = class_dir.join(format!("{}_symmetrized.xyz", candidate.id.0));
            let mut symmetrized = candidate.clone();
            for (atom, position) in symmetrized.atoms.iter_mut().zip(coordinates.iter()) {
                atom.position = *position;
            }
            symmetrized.parameters.insert(
                "symmetrized_by".to_string(),
                serde_json::Value::String("syva".to_string()),
            );
            if let Some(point_group) = &record.point_group {
                symmetrized.parameters.insert(
                    "point_group".to_string(),
                    serde_json::Value::String(point_group.clone()),
                );
            }
            symmetrized.parameters.insert(
                "symmetry_verified".to_string(),
                serde_json::Value::Bool(record.verified),
            );
            write_candidate_xyz(&xyz_path, &symmetrized)?;
            record.xyz_path = Some(xyz_path);
        }
        records.push(record);
    }
    let encoded = records
        .iter()
        .map(serde_json::to_string)
        .collect::<Result<Vec<_>, _>>()?
        .join("\n");
    fs::write(&report_path, format!("{encoded}\n"))?;
    let written = records
        .iter()
        .filter(|record| record.xyz_path.is_some())
        .count();
    append_checkpoint(
        args.out.join("logs").join("checkpoints.jsonl"),
        "symmetrize",
        CheckpointStatus::Passed,
        format!(
            "SYVA symmetrized {} of {} candidates; strict={}",
            written,
            candidates.len(),
            args.strict
        ),
        vec![
            artifact(
                &report_path,
                "jsonl",
                "syva_symmetrization_report",
                Some(records.len()),
            ),
            artifact(
                &args.out,
                "xyz_directory",
                "syva_symmetrized_xyz",
                Some(written),
            ),
        ],
    )?;
    println!(
        "wrote {} SYVA symmetrized xyz files to {}; report {}",
        written,
        args.out.display(),
        report_path.display()
    );
    Ok(())
}

#[derive(Debug, Clone, Default)]
struct GenerationSummary {
    validation_passed: usize,
    validation_failed: usize,
    rejected_degree: usize,
    rejected_chemical_policy: usize,
    mc_rejected_duplicate_wl: usize,
    mc_rejected_similarity: usize,
}

fn generation_summary(candidates: &[crate::MotifCandidate]) -> GenerationSummary {
    let mut summary = GenerationSummary::default();
    for candidate in candidates {
        if let Some(report) = candidate.parameters.get("validation_report") {
            if report
                .get("passed")
                .and_then(serde_json::Value::as_bool)
                .unwrap_or(false)
            {
                summary.validation_passed += 1;
            } else {
                summary.validation_failed += 1;
            }
        }
        if let Some(report) = candidate.parameters.get("constrained_random_report") {
            summary.rejected_degree += report
                .get("rejected_degree")
                .and_then(serde_json::Value::as_u64)
                .unwrap_or(0) as usize;
            summary.rejected_chemical_policy += report
                .get("rejected_chemical_policy")
                .and_then(serde_json::Value::as_u64)
                .unwrap_or(0) as usize;
        }
        if let Some(report) = candidate.parameters.get("topology_mc_report") {
            summary.mc_rejected_duplicate_wl += report
                .get("rejected_duplicate_wl")
                .and_then(serde_json::Value::as_u64)
                .unwrap_or(0) as usize;
            summary.mc_rejected_similarity += report
                .get("rejected_similarity")
                .and_then(serde_json::Value::as_u64)
                .unwrap_or(0) as usize;
        }
    }
    summary
}

fn generation_inputs(
    request: &GenerationRequest,
    summary: &GenerationSummary,
) -> BTreeMap<String, String> {
    BTreeMap::from([
        ("formula".to_string(), request.formula.clone()),
        ("n_min".to_string(), request.n_min.to_string()),
        ("n_max".to_string(), request.n_max.to_string()),
        ("bond_length".to_string(), request.bond_length.to_string()),
        (
            "bond_length_source".to_string(),
            request
                .bond_length_source
                .clone()
                .unwrap_or_else(|| "unspecified".to_string()),
        ),
        ("generators".to_string(), request.generator_names.join(",")),
        (
            "max_candidates".to_string(),
            request.max_candidates.to_string(),
        ),
        (
            "seed".to_string(),
            request
                .seed
                .map(|seed| seed.to_string())
                .unwrap_or_else(|| "none".to_string()),
        ),
        (
            "bond_window".to_string(),
            format!(
                "{}:{}",
                request.constraints.validation.bond_min_factor,
                request.constraints.validation.bond_max_factor
            ),
        ),
        (
            "min_nonbonded_factor".to_string(),
            request
                .constraints
                .validation
                .min_nonbonded_factor
                .to_string(),
        ),
        (
            "edge_policy".to_string(),
            format!("{:?}", request.constraints.edge_policy),
        ),
        (
            "degree_bounds".to_string(),
            format!("{:?}", request.constraints.degree_bounds),
        ),
        (
            "target_extra_edges".to_string(),
            request
                .constraints
                .target_extra_edges
                .map(|value| value.to_string())
                .unwrap_or_else(|| "auto".to_string()),
        ),
        (
            "mc_proposals".to_string(),
            request.constraints.mc_proposals.to_string(),
        ),
        (
            "mc_max_accept".to_string(),
            request.constraints.mc_max_accept.to_string(),
        ),
        (
            "mc_min_jaccard_distance".to_string(),
            request.constraints.mc_min_jaccard_distance.to_string(),
        ),
        (
            "summary_validation_passed".to_string(),
            summary.validation_passed.to_string(),
        ),
        (
            "summary_validation_failed".to_string(),
            summary.validation_failed.to_string(),
        ),
        (
            "summary_rejected_degree".to_string(),
            summary.rejected_degree.to_string(),
        ),
        (
            "summary_rejected_chemical_policy".to_string(),
            summary.rejected_chemical_policy.to_string(),
        ),
        (
            "summary_mc_rejected_duplicate_wl".to_string(),
            summary.mc_rejected_duplicate_wl.to_string(),
        ),
        (
            "summary_mc_rejected_similarity".to_string(),
            summary.mc_rejected_similarity.to_string(),
        ),
    ])
}
