use anyhow::{Context, Result};
use patina_test::{
    as_ga_fixture_dir, compute_native_compatible_hashkey, default_dreadnaut_path,
    discover_xyz_fixtures, infer_builtin_atom_specs, load_xyz_candidate,
};

fn main() -> Result<()> {
    let fixture_dir = as_ga_fixture_dir();
    let dreadnaut_path = default_dreadnaut_path();
    let scratch_dir = std::env::temp_dir().join("patina_test_hashkeys");

    for fixture in discover_xyz_fixtures(&fixture_dir)? {
        let candidate = load_xyz_candidate(&fixture)?;
        let atom_specs = infer_builtin_atom_specs(&candidate)?;
        let fixture_name = fixture
            .file_name()
            .and_then(|name| name.to_str())
            .context("fixture has a non-utf8 file name")?;
        let label = fixture
            .file_stem()
            .and_then(|stem| stem.to_str())
            .context("fixture has a non-utf8 file stem")?;
        let hashkey = compute_native_compatible_hashkey(
            &candidate,
            &atom_specs,
            &dreadnaut_path,
            &scratch_dir,
            label,
        )?;
        println!("{fixture_name} {hashkey}");
    }

    Ok(())
}
