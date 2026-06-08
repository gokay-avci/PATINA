use anyhow::{bail, Result};
use patina_test::{
    as_ga_expected_manifest_path, as_ga_fixture_dir, compute_native_compatible_hashkey,
    default_dreadnaut_path, discover_xyz_fixtures, infer_builtin_atom_specs,
    load_expected_hashkeys, load_xyz_candidate,
};
use std::collections::BTreeMap;

#[test]
#[ignore = "requires a local dreadnaut executable and a native expected-hashkey manifest"]
fn as_ga_native_hashkeys_match_expected_manifest() -> Result<()> {
    let manifest_path = as_ga_expected_manifest_path();
    if !manifest_path.exists() {
        bail!(
            "missing expected native hashkey manifest `{}`",
            manifest_path.display()
        );
    }

    let expected_rows = load_expected_hashkeys(&manifest_path)?;
    let expected_by_name = expected_rows
        .into_iter()
        .map(|row| (row.fixture_name, row.hashkey))
        .collect::<BTreeMap<_, _>>();
    let fixtures = discover_xyz_fixtures(&as_ga_fixture_dir())?;
    let dreadnaut_path = default_dreadnaut_path();
    let scratch_dir = std::env::temp_dir().join("patina_test_parity");

    let mut missing = Vec::new();
    let mut mismatches = Vec::new();
    for fixture in fixtures {
        let fixture_name = fixture
            .file_name()
            .and_then(|name| name.to_str())
            .ok_or_else(|| anyhow::anyhow!("fixture has a non-utf8 file name"))?
            .to_string();
        let Some(expected_hashkey) = expected_by_name.get(&fixture_name) else {
            missing.push(fixture_name);
            continue;
        };
        let candidate = load_xyz_candidate(&fixture)?;
        let atom_specs = infer_builtin_atom_specs(&candidate)?;
        let observed = compute_native_compatible_hashkey(
            &candidate,
            &atom_specs,
            &dreadnaut_path,
            &scratch_dir,
            fixture
                .file_stem()
                .and_then(|stem| stem.to_str())
                .ok_or_else(|| anyhow::anyhow!("fixture has a non-utf8 file stem"))?,
        )?;
        if &observed != expected_hashkey {
            mismatches.push(format!(
                "{} expected={} observed={}",
                fixture_name, expected_hashkey, observed
            ));
        }
    }

    if !missing.is_empty() || !mismatches.is_empty() {
        let mut message = String::new();
        if !missing.is_empty() {
            message.push_str("fixtures missing expected hashkeys:\n");
            for row in missing {
                message.push_str(&format!("  {row}\n"));
            }
        }
        if !mismatches.is_empty() {
            message.push_str("hashkey mismatches:\n");
            for row in mismatches {
                message.push_str(&format!("  {row}\n"));
            }
        }
        bail!(message.trim_end().to_string());
    }

    Ok(())
}
