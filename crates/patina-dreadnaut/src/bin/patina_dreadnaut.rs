use anyhow::{Context, Result};
use clap::Parser;
use patina_dreadnaut::{
    canonical_hashkey_from_graph_file, export_dreadnaut_graph, infer_atom_specs_for_candidate,
    load_xyz_candidate, parse_atoms_file, resolve_dreadnaut_path,
};
use std::path::PathBuf;

#[derive(Debug, Parser)]
#[command(name = "patina-dreadnaut")]
struct Cli {
    /// Input XYZ structure file.
    #[arg(long)]
    xyz: PathBuf,

    /// Optional atoms.in override file used for species ordering and radii.
    #[arg(long)]
    atoms: Option<PathBuf>,

    /// Output graph file. Defaults to <xyz>.dreadnaut next to the input.
    #[arg(long)]
    output: Option<PathBuf>,

    /// Radius mode such as IR or CR.
    #[arg(long, default_value = "IR")]
    radius_mode: String,

    /// Additive constant used in the hashkey radius criterion.
    #[arg(long, default_value_t = 0.0)]
    radius_const: f64,

    /// Optional direct dreadnaut binary to run after graph export.
    #[arg(long)]
    dreadnaut: Option<PathBuf>,
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    let candidate = load_xyz_candidate(&cli.xyz)?;
    let atom_specs_override = cli.atoms.as_deref().map(parse_atoms_file).transpose()?;
    let atom_specs = infer_atom_specs_for_candidate(&candidate, atom_specs_override.as_deref())?;
    let output_path = cli
        .output
        .unwrap_or_else(|| cli.xyz.with_extension("dreadnaut"));

    export_dreadnaut_graph(
        &candidate,
        &atom_specs,
        &cli.radius_mode,
        cli.radius_const,
        &output_path,
    )?;
    println!("graph={}", output_path.display());

    if cli.dreadnaut.is_some() {
        let dreadnaut_path = resolve_dreadnaut_path(cli.dreadnaut.as_deref())?;
        let hashkey = canonical_hashkey_from_graph_file(&dreadnaut_path, &output_path)
            .with_context(|| format!("failed to canonicalize `{}`", output_path.display()))?;
        println!("dreadnaut={}", dreadnaut_path.display());
        println!("hashkey={hashkey}");
    }

    Ok(())
}
