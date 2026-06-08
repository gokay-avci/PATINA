use anyhow::{Context, Result};
use patina_dreadnaut::export_dreadnaut_graph;
use patina_test::{
    as_ga_fixture_dir, discover_xyz_fixtures, infer_builtin_atom_specs, load_xyz_candidate,
};

fn main() -> Result<()> {
    let output_dir = std::env::temp_dir().join("patina_test_as_ga_graphs");
    std::fs::create_dir_all(&output_dir)
        .with_context(|| format!("failed to create `{}`", output_dir.display()))?;

    for fixture in discover_xyz_fixtures(&as_ga_fixture_dir())? {
        let candidate = load_xyz_candidate(&fixture)?;
        let atom_specs = infer_builtin_atom_specs(&candidate)?;
        let file_stem = fixture
            .file_stem()
            .and_then(|stem| stem.to_str())
            .context("fixture has a non-utf8 file stem")?;
        let output_path = output_dir.join(format!("{file_stem}.dreadnaut"));
        export_dreadnaut_graph(&candidate, &atom_specs, "IR", 0.0, &output_path)?;
        println!("{}", output_path.display());
    }

    Ok(())
}
