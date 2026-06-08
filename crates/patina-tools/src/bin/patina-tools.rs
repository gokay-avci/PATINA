use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
use csv::WriterBuilder;
use patina_tools::analysis::cluster_symmetry::{
    run_cluster_symmetry_workflow, symmetrized_xyz_structure, ClusterSymmetryConfig,
};
use patina_tools::analysis::thermally_averaged::{
    parse_temperature_list, run_thermally_averaged_workflow, ThermalAverageWorkflowReport,
};
use patina_tools::analysis::unique_structures::{
    run_unique_structures_workflow, write_unique_structures_outputs, UniqueStructuresConfig,
    UniqueStructuresReport,
};
use patina_tools::genetic_algorithm::energy_evolution::{
    run_energy_evolution_workflow, GaEnergyEvolutionConfig,
};
use patina_tools::genetic_algorithm::family_tree::{
    run_family_tree_workflow, FamilyTreeConfig, FamilyTreeReport,
};
use patina_tools::genetic_algorithm::native_gm_iter::{
    run_native_gm_iter_workflow, NativeGmIterConfig,
};
use patina_tools::genetic_algorithm::native_run_histogram::{
    run_energy_histogram_workflow, GaEnergyHistogramConfig, GaEnergyHistogramReport,
};
use patina_tools::io::legacy_xyz::write_legacy_xyz;
use patina_tools::mining::surface_energy::{
    compute_cluster_surface_energy, ClusterSurfaceEnergyRequest,
};
use patina_tools::plot::svg::{
    write_histogram_chart_svg, write_line_chart_svg, SvgChartSpec, SvgHistogramBin, SvgLineSeries,
};
use serde::Serialize;
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Debug, Parser)]
#[command(name = "patina-tools")]
#[command(about = "Rust-native post-processing tools for PATINA tracked runs")]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Debug, Subcommand)]
enum Command {
    /// Summarize GA energy evolution and optionally export CSV for plotting.
    EnergyEvolution(EnergyEvolutionArgs),
    /// Summarize the current population energy histogram from the native checkpoint.
    EnergyHistogram(EnergyHistogramArgs),
    /// Reconstruct the retained family tree and export graph-friendly artifacts.
    FamilyTree(FamilyTreeArgs),
    /// Locate the first generation where the final best canonical hashkey appears.
    NativeGmIter(NativeGmIterArgs),
    /// Compute thermally averaged statistics from energy/property CSV input.
    ThermallyAveraged(ThermallyAveragedArgs),
    /// Deduplicate structures by canonical hashkey and export unique structure outputs.
    UniqueStructures(UniqueStructuresArgs),
    /// Compute cluster surface energy from scalar geometry/energy inputs.
    SurfaceEnergy(SurfaceEnergyArgs),
    /// Analyze 0D cluster point-group symmetry from an XYZ structure via patina-syva.
    ClusterSymmetry(ClusterSymmetryArgs),
}

#[derive(Debug, clap::Args)]
struct EnergyEvolutionArgs {
    #[arg(long)]
    run_dir: PathBuf,
    #[arg(long)]
    json_out: Option<PathBuf>,
    #[arg(long)]
    points_csv_out: Option<PathBuf>,
    #[arg(long)]
    svg_out: Option<PathBuf>,
}

#[derive(Debug, clap::Args)]
struct EnergyHistogramArgs {
    #[arg(long)]
    run_dir: PathBuf,
    #[arg(long, default_value_t = 12)]
    bin_count: usize,
    #[arg(long)]
    json_out: Option<PathBuf>,
    #[arg(long)]
    svg_out: Option<PathBuf>,
}

#[derive(Debug, clap::Args)]
struct FamilyTreeArgs {
    #[arg(long)]
    run_dir: PathBuf,
    #[arg(long)]
    json_out: Option<PathBuf>,
    #[arg(long)]
    dot_out: Option<PathBuf>,
    #[arg(long)]
    nodes_csv_out: Option<PathBuf>,
    #[arg(long)]
    edges_csv_out: Option<PathBuf>,
}

#[derive(Debug, clap::Args)]
struct NativeGmIterArgs {
    #[arg(long)]
    run_dir: PathBuf,
    #[arg(long)]
    json_out: Option<PathBuf>,
}

#[derive(Debug, clap::Args)]
struct ThermallyAveragedArgs {
    #[arg(long)]
    input_csv: PathBuf,
    #[arg(long, default_value = "293")]
    temperatures: String,
    #[arg(long)]
    unique_mode: bool,
    #[arg(long)]
    json_out: Option<PathBuf>,
    #[arg(long)]
    points_csv_out: Option<PathBuf>,
    #[arg(long)]
    svg_out: Option<PathBuf>,
}

#[derive(Debug, clap::Args)]
struct UniqueStructuresArgs {
    #[arg(long)]
    input_root: PathBuf,
    #[arg(long, default_value = "xyz")]
    extension: String,
    #[arg(long)]
    atoms: Option<PathBuf>,
    #[arg(long)]
    dreadnaut_path: Option<PathBuf>,
    #[arg(long, default_value = "IR")]
    hashkey_radius_mode: String,
    #[arg(long, default_value_t = 3.34)]
    hashkey_radius_const: f64,
    #[arg(long)]
    output_dir: Option<PathBuf>,
    #[arg(long)]
    json_out: Option<PathBuf>,
    #[arg(long)]
    svg_out: Option<PathBuf>,
}

#[derive(Debug, clap::Args)]
struct SurfaceEnergyArgs {
    #[arg(long)]
    total_energy: f64,
    #[arg(long)]
    atom_count: usize,
    #[arg(long)]
    bulk_energy_per_atom: f64,
    #[arg(long)]
    area: f64,
    #[arg(long)]
    json_out: Option<PathBuf>,
}

#[derive(Debug, clap::Args)]
struct ClusterSymmetryArgs {
    #[arg(long)]
    xyz: PathBuf,
    #[arg(long)]
    json_out: Option<PathBuf>,
    #[arg(long)]
    subgroup: Option<String>,
    #[arg(long)]
    symmetrize: bool,
    #[arg(long)]
    strict_symmetrize: bool,
    #[arg(long)]
    symmetrized_xyz_out: Option<PathBuf>,
    #[arg(long, default_value_t = 0.001)]
    tolerance: f64,
    #[arg(long, default_value_t = 5.0e-2)]
    tolerance_upper: f64,
    #[arg(long, default_value_t = 5.0e-3)]
    tolerance_lower: f64,
    #[arg(long)]
    skip_tolerance_scan: bool,
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    match cli.command {
        Command::EnergyEvolution(args) => run_energy_evolution(args),
        Command::EnergyHistogram(args) => run_energy_histogram(args),
        Command::FamilyTree(args) => run_family_tree(args),
        Command::NativeGmIter(args) => run_native_gm_iter(args),
        Command::ThermallyAveraged(args) => run_thermally_averaged(args),
        Command::UniqueStructures(args) => run_unique_structures(args),
        Command::SurfaceEnergy(args) => run_surface_energy(args),
        Command::ClusterSymmetry(args) => run_cluster_symmetry(args),
    }
}

fn run_energy_evolution(args: EnergyEvolutionArgs) -> Result<()> {
    let report = run_energy_evolution_workflow(&GaEnergyEvolutionConfig {
        run_dir: args.run_dir,
    })?;
    if let Some(path) = args.json_out.as_deref() {
        write_json(path, &report)?;
    }
    if let Some(path) = args.points_csv_out.as_deref() {
        ensure_parent_dir(path)?;
        let mut writer = WriterBuilder::new()
            .has_headers(true)
            .from_path(path)
            .with_context(|| format!("failed to open csv writer `{}`", path.display()))?;
        for point in &report.points {
            writer.serialize(point).with_context(|| {
                format!(
                    "failed to serialize energy evolution row into `{}`",
                    path.display()
                )
            })?;
        }
        writer
            .flush()
            .with_context(|| format!("failed to flush csv writer `{}`", path.display()))?;
    }
    if let Some(path) = args.svg_out.as_deref() {
        ensure_parent_dir(path)?;
        write_energy_evolution_svg(path, &report)?;
    }
    print_json(&report)
}

fn run_energy_histogram(args: EnergyHistogramArgs) -> Result<()> {
    let report = run_energy_histogram_workflow(&GaEnergyHistogramConfig {
        run_dir: args.run_dir,
        bin_count: args.bin_count,
    })?;
    if let Some(path) = args.json_out.as_deref() {
        write_json(path, &report)?;
    }
    if let Some(path) = args.svg_out.as_deref() {
        ensure_parent_dir(path)?;
        write_energy_histogram_svg(path, &report)?;
    }
    print_json(&report)
}

fn run_family_tree(args: FamilyTreeArgs) -> Result<()> {
    let report = run_family_tree_workflow(&FamilyTreeConfig {
        run_dir: args.run_dir,
    })?;
    if let Some(path) = args.json_out.as_deref() {
        write_json(path, &report)?;
    }
    if let Some(path) = args.nodes_csv_out.as_deref() {
        write_family_tree_nodes_csv(path, &report)?;
    }
    if let Some(path) = args.edges_csv_out.as_deref() {
        write_family_tree_edges_csv(path, &report)?;
    }
    if let Some(path) = args.dot_out.as_deref() {
        write_family_tree_dot(path, &report)?;
    }
    print_json(&report)
}

fn run_native_gm_iter(args: NativeGmIterArgs) -> Result<()> {
    let report = run_native_gm_iter_workflow(&NativeGmIterConfig {
        run_dir: args.run_dir,
    })?;
    if let Some(path) = args.json_out.as_deref() {
        write_json(path, &report)?;
    }
    print_json(&report)
}

fn run_thermally_averaged(args: ThermallyAveragedArgs) -> Result<()> {
    let temperatures =
        parse_temperature_list(&args.temperatures).context("failed to parse `--temperatures`")?;
    let report = run_thermally_averaged_workflow(&args.input_csv, &temperatures, args.unique_mode)
        .context("failed to compute thermally averaged statistics")?;
    if let Some(path) = args.json_out.as_deref() {
        write_json(path, &report)?;
    }
    if let Some(path) = args.points_csv_out.as_deref() {
        write_thermal_average_points_csv(path, &report)?;
    }
    if let Some(path) = args.svg_out.as_deref() {
        ensure_parent_dir(path)?;
        write_thermal_average_svg(path, &report)?;
    }
    print_json(&report)
}

fn run_unique_structures(args: UniqueStructuresArgs) -> Result<()> {
    let report = run_unique_structures_workflow(&UniqueStructuresConfig {
        input_root: args.input_root,
        extension: args.extension,
        atoms_path: args.atoms,
        dreadnaut_path: args.dreadnaut_path,
        hashkey_radius_mode: args.hashkey_radius_mode,
        hashkey_radius_const: args.hashkey_radius_const,
    })
    .context("failed to compute unique structures")?;
    if let Some(path) = args.json_out.as_deref() {
        write_json(path, &report)?;
    }
    if let Some(path) = args.output_dir.as_deref() {
        write_unique_structures_outputs(&report, path)
            .with_context(|| format!("failed to write unique outputs in `{}`", path.display()))?;
    }
    if let Some(path) = args.svg_out.as_deref() {
        ensure_parent_dir(path)?;
        write_unique_structures_svg(path, &report)?;
    }
    print_json(&report)
}

fn run_surface_energy(args: SurfaceEnergyArgs) -> Result<()> {
    let summary = compute_cluster_surface_energy(&ClusterSurfaceEnergyRequest {
        total_energy: args.total_energy,
        atom_count: args.atom_count,
        bulk_energy_per_atom: args.bulk_energy_per_atom,
        area: args.area,
    })
    .context("failed to compute surface energy")?;
    if let Some(path) = args.json_out.as_deref() {
        write_json(path, &summary)?;
    }
    print_json(&summary)
}

fn run_cluster_symmetry(args: ClusterSymmetryArgs) -> Result<()> {
    if args.strict_symmetrize && !args.symmetrize {
        anyhow::bail!("`--strict-symmetrize` requires `--symmetrize`");
    }
    if (args.symmetrize || args.symmetrized_xyz_out.is_some()) && args.subgroup.is_none() {
        anyhow::bail!(
            "`--subgroup <LABEL>` is required when using `--symmetrize` or `--symmetrized-xyz-out`"
        );
    }
    if args.symmetrized_xyz_out.is_some() && !args.symmetrize {
        anyhow::bail!("`--symmetrized-xyz-out` requires `--symmetrize`");
    }

    let report = run_cluster_symmetry_workflow(&ClusterSymmetryConfig {
        input_xyz: args.xyz,
        tolerance: args.tolerance,
        tolerance_upper: args.tolerance_upper,
        tolerance_lower: args.tolerance_lower,
        include_tolerance_scan: !args.skip_tolerance_scan,
        selected_subgroup: args.subgroup,
        symmetrize: args.symmetrize,
        strict_symmetrize: args.strict_symmetrize,
    })?;
    if let Some(path) = args.symmetrized_xyz_out.as_deref() {
        let structure = symmetrized_xyz_structure(&report)
            .context("symmetrized xyz requested but no symmetrized geometry was produced")?;
        ensure_parent_dir(path)?;
        write_legacy_xyz(&structure, path)
            .with_context(|| format!("failed to write `{}`", path.display()))?;
    }
    let json_report = report.json_report();
    if let Some(path) = args.json_out.as_deref() {
        write_json(path, &json_report)?;
    }
    print_json(&json_report)
}

fn write_family_tree_dot(path: &Path, report: &FamilyTreeReport) -> Result<()> {
    ensure_parent_dir(path)?;
    let mut dot = String::from("digraph ga_family_tree {\n");
    dot.push_str("  rankdir=LR;\n");
    dot.push_str("  graph [fontname=\"Helvetica\"];\n");
    dot.push_str("  node [shape=box, style=\"rounded,filled\", fontname=\"Helvetica\", fillcolor=\"#f8fafc\", color=\"#334155\"];\n");
    dot.push_str("  edge [color=\"#64748b\"];\n");

    for node in &report.nodes {
        let fill = if node.is_final_population {
            "#dbeafe"
        } else if node.parent_labels.is_empty() {
            "#dcfce7"
        } else {
            "#f8fafc"
        };
        let hash = node
            .canonical_hashkey
            .as_deref()
            .map(short_hashkey)
            .unwrap_or("none");
        let label = format!(
            "g{generation:04} m{member:04}\\n{source}\\norigin={origin}\\nhash={hash}",
            generation = node.generation,
            member = node.member_id,
            source = escape_dot_label(&node.source_label),
            origin = escape_dot_label(&node.origin),
            hash = escape_dot_label(hash),
        );
        dot.push_str(&format!(
            "  \"{node_id}\" [label=\"{label}\", fillcolor=\"{fill}\"];\n",
            node_id = escape_dot_id(&node.node_id),
            label = label,
            fill = fill
        ));
    }

    for edge in &report.edges {
        match &edge.parent_node_id {
            Some(parent_id) => {
                dot.push_str(&format!(
                    "  \"{parent}\" -> \"{child}\";\n",
                    parent = escape_dot_id(parent_id),
                    child = escape_dot_id(&edge.child_node_id),
                ));
            }
            None => {
                let missing_id = format!(
                    "missing_parent::{}::{}",
                    edge.child_node_id, edge.parent_label
                );
                let missing_label =
                    format!("unresolved\\n{}", escape_dot_label(&edge.parent_label));
                dot.push_str(&format!(
                    "  \"{missing}\" [label=\"{label}\", shape=ellipse, style=\"dashed,filled\", fillcolor=\"#fee2e2\", color=\"#991b1b\"];\n",
                    missing = escape_dot_id(&missing_id),
                    label = missing_label,
                ));
                dot.push_str(&format!(
                    "  \"{missing}\" -> \"{child}\" [style=dashed, color=\"#b91c1c\"];\n",
                    missing = escape_dot_id(&missing_id),
                    child = escape_dot_id(&edge.child_node_id),
                ));
            }
        }
    }

    dot.push_str("}\n");
    fs::write(path, dot).with_context(|| format!("failed to write `{}`", path.display()))
}

fn write_energy_evolution_svg(
    path: &Path,
    report: &patina_tools::genetic_algorithm::energy_evolution::GaEnergyEvolutionReport,
) -> Result<()> {
    let mut series = Vec::new();

    let best = report
        .points
        .iter()
        .filter_map(|point| {
            point
                .best_energy
                .map(|energy| (point.generation as f64, energy))
        })
        .collect::<Vec<_>>();
    if !best.is_empty() {
        series.push(SvgLineSeries {
            label: "Best".into(),
            color: "#0f766e".into(),
            points: best,
        });
    }

    let mean = report
        .points
        .iter()
        .filter_map(|point| {
            point
                .mean_energy
                .map(|energy| (point.generation as f64, energy))
        })
        .collect::<Vec<_>>();
    if !mean.is_empty() {
        series.push(SvgLineSeries {
            label: "Mean".into(),
            color: "#2563eb".into(),
            points: mean,
        });
    }

    let worst = report
        .points
        .iter()
        .filter_map(|point| {
            point
                .worst_energy
                .map(|energy| (point.generation as f64, energy))
        })
        .collect::<Vec<_>>();
    if !worst.is_empty() {
        series.push(SvgLineSeries {
            label: "Worst".into(),
            color: "#dc2626".into(),
            points: worst,
        });
    }

    write_line_chart_svg(
        path,
        &SvgChartSpec {
            title: "GA Energy Evolution".into(),
            subtitle: Some(format!(
                "{} | backend={} | generations={}",
                report.workflow_owner, report.backend, report.generation_count
            )),
            x_label: "Generation".into(),
            y_label: "Energy (eV)".into(),
        },
        &series,
    )
    .with_context(|| format!("failed to write `{}`", path.display()))
}

fn write_energy_histogram_svg(path: &Path, report: &GaEnergyHistogramReport) -> Result<()> {
    let bins = report
        .bins
        .iter()
        .map(|bin| SvgHistogramBin {
            lower_bound: bin.lower_bound,
            upper_bound: bin.upper_bound,
            count: bin.count,
        })
        .collect::<Vec<_>>();
    write_histogram_chart_svg(
        path,
        &SvgChartSpec {
            title: "GA Energy Histogram".into(),
            subtitle: Some(format!(
                "{} | generation={} | converged={}/{}",
                report.workflow_owner,
                report.generation,
                report.converged_population_size,
                report.population_size
            )),
            x_label: "Energy (eV)".into(),
            y_label: "Count".into(),
        },
        &bins,
        "#7c3aed",
    )
    .with_context(|| format!("failed to write `{}`", path.display()))
}

fn write_thermal_average_points_csv(
    path: &Path,
    report: &ThermalAverageWorkflowReport,
) -> Result<()> {
    ensure_parent_dir(path)?;
    let mut writer = WriterBuilder::new()
        .has_headers(true)
        .from_path(path)
        .with_context(|| format!("failed to open csv writer `{}`", path.display()))?;
    for point in &report.points {
        writer.serialize(point).with_context(|| {
            format!(
                "failed to serialize thermal-average row into `{}`",
                path.display()
            )
        })?;
    }
    writer
        .flush()
        .with_context(|| format!("failed to flush csv writer `{}`", path.display()))
}

fn write_thermal_average_svg(path: &Path, report: &ThermalAverageWorkflowReport) -> Result<()> {
    let value_series = SvgLineSeries {
        label: "Thermal average".into(),
        color: "#b45309".into(),
        points: report
            .points
            .iter()
            .map(|point| (point.temperature_kelvin, point.value))
            .collect(),
    };
    write_line_chart_svg(
        path,
        &SvgChartSpec {
            title: "Thermally Averaged Statistic".into(),
            subtitle: Some(format!(
                "{} records | occurrences={}",
                report.record_count, report.use_occurrences
            )),
            x_label: "Temperature (K)".into(),
            y_label: "Value".into(),
        },
        &[value_series],
    )
    .with_context(|| format!("failed to write `{}`", path.display()))
}

fn write_unique_structures_svg(path: &Path, report: &UniqueStructuresReport) -> Result<()> {
    let points = report
        .entries
        .iter()
        .filter_map(|entry| entry.energy.map(|energy| (entry.rank as f64, energy)))
        .collect::<Vec<_>>();
    write_line_chart_svg(
        path,
        &SvgChartSpec {
            title: "Unique Structure Energy Ranking".into(),
            subtitle: Some(format!(
                "unique={} | scanned={}",
                report.unique_count, report.scanned_file_count
            )),
            x_label: "Rank".into(),
            y_label: "Energy (eV)".into(),
        },
        &[SvgLineSeries {
            label: "Unique energies".into(),
            color: "#059669".into(),
            points,
        }],
    )
    .with_context(|| format!("failed to write `{}`", path.display()))
}

fn write_family_tree_nodes_csv(path: &Path, report: &FamilyTreeReport) -> Result<()> {
    ensure_parent_dir(path)?;
    let mut writer = WriterBuilder::new()
        .has_headers(true)
        .from_path(path)
        .with_context(|| format!("failed to open csv writer `{}`", path.display()))?;
    writer.write_record([
        "node_id",
        "generation",
        "member_id",
        "source_label",
        "relaxed_label",
        "origin",
        "canonical_hashkey",
        "parent_labels",
        "converged",
        "is_final_population",
    ])?;
    for node in &report.nodes {
        writer.write_record([
            node.node_id.as_str(),
            &node.generation.to_string(),
            &node.member_id.to_string(),
            node.source_label.as_str(),
            node.relaxed_label.as_str(),
            node.origin.as_str(),
            node.canonical_hashkey.as_deref().unwrap_or(""),
            &node.parent_labels.join("|"),
            bool_as_csv(node.converged),
            bool_as_csv(node.is_final_population),
        ])?;
    }
    writer
        .flush()
        .with_context(|| format!("failed to flush csv writer `{}`", path.display()))
}

fn write_family_tree_edges_csv(path: &Path, report: &FamilyTreeReport) -> Result<()> {
    ensure_parent_dir(path)?;
    let mut writer = WriterBuilder::new()
        .has_headers(true)
        .from_path(path)
        .with_context(|| format!("failed to open csv writer `{}`", path.display()))?;
    writer.write_record([
        "child_node_id",
        "parent_label",
        "parent_node_id",
        "resolved",
    ])?;
    for edge in &report.edges {
        writer.write_record([
            edge.child_node_id.as_str(),
            edge.parent_label.as_str(),
            edge.parent_node_id.as_deref().unwrap_or(""),
            bool_as_csv(edge.resolved),
        ])?;
    }
    writer
        .flush()
        .with_context(|| format!("failed to flush csv writer `{}`", path.display()))
}

fn write_json<T>(path: &Path, value: &T) -> Result<()>
where
    T: Serialize,
{
    ensure_parent_dir(path)?;
    let payload = serde_json::to_string_pretty(value)
        .with_context(|| format!("failed to serialize `{}`", path.display()))?;
    fs::write(path, payload).with_context(|| format!("failed to write `{}`", path.display()))
}

fn print_json<T>(value: &T) -> Result<()>
where
    T: Serialize,
{
    println!(
        "{}",
        serde_json::to_string_pretty(value).context("failed to serialize report")?
    );
    Ok(())
}

fn ensure_parent_dir(path: &Path) -> Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("failed to create `{}`", parent.display()))?;
    }
    Ok(())
}

fn bool_as_csv(value: bool) -> &'static str {
    if value {
        "true"
    } else {
        "false"
    }
}

fn short_hashkey(raw: &str) -> &str {
    const MAX_LEN: usize = 16;
    if raw.len() <= MAX_LEN {
        raw
    } else {
        &raw[..MAX_LEN]
    }
}

fn escape_dot_label(value: &str) -> String {
    value.replace('\\', "\\\\").replace('"', "\\\"")
}

fn escape_dot_id(value: &str) -> String {
    escape_dot_label(value)
}
