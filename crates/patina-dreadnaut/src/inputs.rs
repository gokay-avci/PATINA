use anyhow::{anyhow, bail, Context, Result};
use patina_sci_kernel::codec::xyz::candidate_from_xyz_path;
use patina_types::Candidate;
use std::collections::BTreeSet;
use std::fs;
use std::path::Path;

use crate::AtomSpec;

const BUILTIN_ATOM_SPECS_CSV: &str = include_str!("builtin_atom_specs.csv");

pub fn load_xyz_candidate(path: &Path) -> Result<Candidate> {
    candidate_from_xyz_path(path)
        .map_err(|err| anyhow!("failed to decode xyz file `{}`: {err:?}", path.display()))
}

pub fn parse_atoms_file(path: &Path) -> Result<Vec<AtomSpec>> {
    let text = fs::read_to_string(path)
        .with_context(|| format!("failed to read atoms file `{}`", path.display()))?;
    let declared_count = text
        .lines()
        .next()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .and_then(|line| line.parse::<usize>().ok());
    let mut records = Vec::new();
    let mut seen_species = BTreeSet::new();
    for line in text.lines().skip(1) {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        let parts = trimmed.split(',').map(str::trim).collect::<Vec<_>>();
        if parts.len() < 5 {
            continue;
        }
        let species = normalize_species_label(parts[1]);
        if species.is_empty()
            || species.eq_ignore_ascii_case("X")
            || !seen_species.insert(species.clone())
        {
            continue;
        }
        records.push(AtomSpec {
            species,
            covalent_radius: parts[3].parse().with_context(|| {
                format!("failed to parse covalent radius in `{}`", path.display())
            })?,
            ionic_radius: parts[4]
                .parse()
                .with_context(|| format!("failed to parse ionic radius in `{}`", path.display()))?,
        });
    }
    if let Some(count) = declared_count {
        let non_placeholder_declared = text
            .lines()
            .skip(1)
            .filter_map(|line| {
                let parts = line.split(',').map(str::trim).collect::<Vec<_>>();
                (parts.len() >= 2).then(|| normalize_species_label(parts[1]))
            })
            .filter(|species| !species.is_empty() && !species.eq_ignore_ascii_case("X"))
            .collect::<BTreeSet<_>>()
            .len();
        if non_placeholder_declared > count {
            bail!(
                "atoms file `{}` declares {count} species but contains at least {non_placeholder_declared} non-placeholder entries",
                path.display()
            );
        }
    }
    Ok(records)
}

pub fn builtin_atom_specs() -> Result<Vec<AtomSpec>> {
    parse_atom_specs_csv(BUILTIN_ATOM_SPECS_CSV, "builtin atom specs")
}

pub fn infer_atom_specs_for_candidate(
    candidate: &Candidate,
    overrides: Option<&[AtomSpec]>,
) -> Result<Vec<AtomSpec>> {
    let overrides = overrides.unwrap_or(&[]);
    let mut species_order = Vec::<String>::new();
    let mut seen = BTreeSet::new();
    for species in &candidate.species {
        let normalized = normalize_species_label(species);
        if normalized.is_empty()
            || normalized.eq_ignore_ascii_case("X")
            || !seen.insert(normalized.clone())
        {
            continue;
        }
        species_order.push(normalized);
    }
    if species_order.is_empty() {
        bail!(
            "candidate `{}` has no non-placeholder species",
            candidate.label
        );
    }

    let builtin = builtin_atom_specs()?;
    let mut resolved = Vec::with_capacity(species_order.len());
    for species in species_order {
        if let Some(spec) = overrides.iter().find(|record| record.species == species) {
            resolved.push(spec.clone());
            continue;
        }
        let builtin_spec = builtin
            .iter()
            .find(|record| record.species == species)
            .cloned()
            .ok_or_else(|| anyhow!("no built-in atom specification for species `{species}`"))?;
        resolved.push(builtin_spec);
    }
    Ok(resolved)
}

pub(crate) fn normalize_species_label(raw: &str) -> String {
    let mut label = raw.trim();
    if let Some((head, _)) = label.split_once(char::is_whitespace) {
        label = head;
    }
    for suffix in [
        "_core", "_shel", "_shell", "-core", "-shel", "-shell", ".core", ".shel", ".shell",
    ] {
        if label.len() > suffix.len() && label.ends_with(suffix) {
            label = &label[..label.len() - suffix.len()];
            break;
        }
    }
    label.trim().to_string()
}

fn parse_atom_specs_csv(text: &str, source_label: &str) -> Result<Vec<AtomSpec>> {
    let mut records = Vec::new();
    let mut seen_species = BTreeSet::new();
    for line in text.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        let parts = trimmed.split(',').map(str::trim).collect::<Vec<_>>();
        if parts.len() < 3 {
            continue;
        }
        let species = normalize_species_label(parts[0]);
        if species.is_empty()
            || species.eq_ignore_ascii_case("X")
            || !seen_species.insert(species.clone())
        {
            continue;
        }
        records.push(AtomSpec {
            species,
            covalent_radius: parts[1]
                .parse()
                .with_context(|| format!("failed to parse covalent radius in {source_label}"))?,
            ionic_radius: parts[2]
                .parse()
                .with_context(|| format!("failed to parse ionic radius in {source_label}"))?,
        });
    }
    Ok(records)
}

#[cfg(test)]
mod tests {
    use super::parse_atoms_file;
    use std::fs;

    #[test]
    fn parse_atoms_file_skips_placeholder_and_deduplicates_species() {
        let path =
            std::env::temp_dir().join(format!("patina_dreadnaut_atoms_{}.in", std::process::id()));
        fs::write(
            &path,
            "4\n31,Ga,69.7230,1.2600,0.6200\n33,As_shell,74.9216,1.1900,1.1900\n107,X,0.0,0.0,0.0\n31,Ga_core,69.7230,1.2600,0.6200\n",
        )
        .expect("write atoms file");
        let parsed = parse_atoms_file(&path).expect("parse atoms");
        fs::remove_file(&path).ok();
        assert_eq!(parsed.len(), 2);
        assert_eq!(parsed[0].species, "Ga");
        assert_eq!(parsed[1].species, "As");
    }
}
