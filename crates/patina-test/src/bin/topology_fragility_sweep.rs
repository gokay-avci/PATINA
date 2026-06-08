use anyhow::{anyhow, bail, Context, Result};
use patina_dreadnaut::{infer_atom_specs_for_candidate, parse_atoms_file};
use patina_perturber::{
    perturbation_topology_sweep, topology_sweep_csv, PerturbationConfig,
    PerturbationTopologySweepRequest,
};
use patina_test::{default_dreadnaut_path, load_xyz_cluster_structure};
use std::fs;
use std::path::{Path, PathBuf};

fn main() -> Result<()> {
    let args = std::env::args().skip(1).collect::<Vec<_>>();
    let config = CliConfig::parse(&args)?;
    let source = load_xyz_cluster_structure(&config.xyz_path)?;
    let source_candidate = patina_types::Candidate::from(&source);
    let atom_specs_override = config
        .atoms_path
        .as_deref()
        .map(parse_atoms_file)
        .transpose()?;
    let atom_specs =
        infer_atom_specs_for_candidate(&source_candidate, atom_specs_override.as_deref())?;
    let sweep = perturbation_topology_sweep(
        &source,
        &PerturbationTopologySweepRequest {
            template_config: PerturbationConfig {
                sigma: config.sigmas[0],
                max_displacement: config.max_displacement,
                validate_min_distance: config.min_distance,
                seed: config.seed,
                ..PerturbationConfig::default()
            },
            sigmas: config.sigmas,
            repeats_per_sigma: config.repeats,
            atom_specs,
            radius_mode: config.radius_mode,
            radius_const: config.radius_const,
            include_p_orbitals: config.include_p_orbitals,
            margin_epsilon: config.margin_epsilon,
            dreadnaut_path: Some(config.dreadnaut_path),
        },
    )?;

    let csv = topology_sweep_csv(&sweep);
    fs::write(&config.output_path, csv)
        .with_context(|| format!("failed to write `{}`", config.output_path.display()))?;
    println!("{}", config.output_path.display());
    Ok(())
}

#[derive(Debug, Clone)]
struct CliConfig {
    xyz_path: PathBuf,
    output_path: PathBuf,
    atoms_path: Option<PathBuf>,
    dreadnaut_path: PathBuf,
    sigmas: Vec<f64>,
    repeats: usize,
    radius_mode: String,
    radius_const: f64,
    margin_epsilon: f64,
    max_displacement: Option<f64>,
    min_distance: Option<f64>,
    include_p_orbitals: bool,
    seed: Option<u64>,
}

impl CliConfig {
    fn parse(args: &[String]) -> Result<Self> {
        let mut xyz_path = None;
        let mut output_path = None;
        let mut atoms_path = None;
        let mut dreadnaut_path = default_dreadnaut_path();
        let mut sigmas = vec![0.01, 0.025, 0.05, 0.1];
        let mut repeats = 20usize;
        let mut radius_mode = String::from("IR");
        let mut radius_const = 0.4_f64;
        let mut margin_epsilon = 0.1_f64;
        let mut max_displacement = None;
        let mut min_distance = None;
        let mut include_p_orbitals = false;
        let mut seed = Some(11_u64);

        let mut index = 0usize;
        while index < args.len() {
            match args[index].as_str() {
                "--xyz" => {
                    xyz_path = Some(PathBuf::from(next_value(args, &mut index, "--xyz")?));
                }
                "--output" => {
                    output_path = Some(PathBuf::from(next_value(args, &mut index, "--output")?));
                }
                "--atoms" => {
                    atoms_path = Some(PathBuf::from(next_value(args, &mut index, "--atoms")?));
                }
                "--dreadnaut" => {
                    dreadnaut_path = PathBuf::from(next_value(args, &mut index, "--dreadnaut")?);
                }
                "--sigmas" => {
                    sigmas = next_value(args, &mut index, "--sigmas")?
                        .split(',')
                        .map(|part| {
                            part.parse::<f64>()
                                .with_context(|| format!("invalid sigma `{part}`"))
                        })
                        .collect::<Result<Vec<_>>>()?;
                    if sigmas.is_empty() {
                        bail!("--sigmas must contain at least one value");
                    }
                }
                "--repeats" => {
                    repeats = next_value(args, &mut index, "--repeats")?
                        .parse()
                        .context("invalid --repeats value")?;
                }
                "--radius-mode" => {
                    radius_mode = next_value(args, &mut index, "--radius-mode")?.to_string();
                }
                "--radius-const" => {
                    radius_const = next_value(args, &mut index, "--radius-const")?
                        .parse()
                        .context("invalid --radius-const value")?;
                }
                "--margin-epsilon" => {
                    margin_epsilon = next_value(args, &mut index, "--margin-epsilon")?
                        .parse()
                        .context("invalid --margin-epsilon value")?;
                }
                "--max-displacement" => {
                    max_displacement = Some(
                        next_value(args, &mut index, "--max-displacement")?
                            .parse()
                            .context("invalid --max-displacement value")?,
                    );
                }
                "--min-distance" => {
                    min_distance = Some(
                        next_value(args, &mut index, "--min-distance")?
                            .parse()
                            .context("invalid --min-distance value")?,
                    );
                }
                "--include-p-orbitals" => {
                    include_p_orbitals = true;
                }
                "--seed" => {
                    seed = Some(
                        next_value(args, &mut index, "--seed")?
                            .parse()
                            .context("invalid --seed value")?,
                    );
                }
                "--no-seed" => {
                    seed = None;
                }
                "--help" | "-h" => {
                    print_help();
                    std::process::exit(0);
                }
                other => bail!("unrecognized argument `{other}`"),
            }
            index += 1;
        }

        let xyz_path = xyz_path.ok_or_else(|| anyhow!("--xyz is required"))?;
        let output_path = output_path.unwrap_or_else(|| default_output_path(&xyz_path));
        Ok(Self {
            xyz_path,
            output_path,
            atoms_path,
            dreadnaut_path,
            sigmas,
            repeats,
            radius_mode,
            radius_const,
            margin_epsilon,
            max_displacement,
            min_distance,
            include_p_orbitals,
            seed,
        })
    }
}

fn next_value<'a>(args: &'a [String], index: &mut usize, flag: &str) -> Result<&'a str> {
    *index += 1;
    args.get(*index)
        .map(String::as_str)
        .ok_or_else(|| anyhow!("{flag} requires a value"))
}

fn default_output_path(xyz_path: &Path) -> PathBuf {
    let stem = xyz_path
        .file_stem()
        .and_then(|value| value.to_str())
        .unwrap_or("topology_sweep");
    xyz_path.with_file_name(format!("{stem}_topology_fragility.csv"))
}

fn print_help() {
    eprintln!(
        "Usage: cargo run -p patina-test --bin topology_fragility_sweep -- --xyz <file.xyz> [options]\n\
         \n\
         Options:\n\
         --output <path>             CSV output path\n\
         --atoms <path>              optional atoms.in override path\n\
         --dreadnaut <path>          dreadnaut binary path\n\
         --sigmas <a,b,c>            comma-separated sigma values\n\
         --repeats <n>               perturbations per sigma\n\
         --radius-mode <IR|CR>       hashkey radius mode\n\
         --radius-const <x>          additive hashkey radius constant\n\
         --margin-epsilon <x>        near-critical cutoff threshold\n\
         --max-displacement <x>      optional perturbation clamp\n\
         --min-distance <x>          optional minimum interatomic distance\n\
         --include-p-orbitals        enable p orbitals in overlap fingerprint\n\
         --seed <n>                  deterministic base seed\n\
         --no-seed                   disable deterministic seeding"
    );
}
