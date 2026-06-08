use anyhow::{anyhow, Context, Result};
use patina_sci_kernel::codec::cif::candidate_from_cif_path;
use patina_sci_kernel::codec::xyz::{
    candidate_from_extxyz_path, parse_extxyz_lattice, parse_extxyz_pbc,
};
use patina_search::{
    AtomSpec as SearchAtomSpec, HashkeyRadiusMode, TopologyAtom as SearchTopologyAtom,
};
use patina_types::StructureDimensionality;
use std::collections::{BTreeMap, HashMap};
use std::fs;
use std::path::{Path, PathBuf};

use super::scott_topology_types::AtomSpecRecord;

pub fn build_topology_nearness_report(
    request: &crate::ResolvedScottSearchRun,
    raw_dir: &Path,
    structures_dir: &Path,
) -> Result<Option<crate::TopologyNearnessReport>> {
    if !request.shared.use_top_analysis.unwrap_or(false) {
        return Ok(None);
    }

    let atoms_path = raw_dir.join("atoms.in");
    if !atoms_path.exists() {
        return Ok(None);
    }
    let atom_specs = parse_atoms_file(&atoms_path)?;
    if atom_specs.is_empty() {
        return Ok(None);
    }

    let species_order = atom_specs
        .iter()
        .filter(|record| !record.species.eq_ignore_ascii_case("X"))
        .map(|record| record.species.clone())
        .collect::<Vec<_>>();
    let radius_mode = request
        .shared
        .hashkey_radius
        .clone()
        .unwrap_or_else(|| "IR".to_string());
    let radius_const = request.shared.hashkey_radius_const.unwrap_or(0.0);

    let structure_index = index_structure_snapshots(structures_dir)?;
    let hashkey_index = collect_hashkey_index(raw_dir)?;
    let pair_requests = collect_topology_pair_requests(raw_dir)?;
    if pair_requests.is_empty() {
        return Ok(Some(crate::TopologyNearnessReport {
            metadata: crate::TopologyNearnessMetadata {
                radius_mode,
                radius_const,
                species_order,
                near_edge_threshold: crate::TOPOLOGY_NEAR_EDGE_THRESHOLD,
                near_coordination_threshold: crate::TOPOLOGY_NEAR_COORDINATION_THRESHOLD,
                sources: crate::TopologyNearnessSources {
                    audit_events: raw_dir
                        .join("hashkey_audit_events.csv")
                        .exists()
                        .then(|| "raw/hashkey_audit_events.csv".to_string()),
                    top_structures_hashkeys: raw_dir
                        .join("top_structures_hashkeys")
                        .exists()
                        .then(|| "raw/top_structures_hashkeys".to_string()),
                    ga_statistics: collect_ga_statistics_paths(raw_dir),
                },
            },
            summary: crate::TopologyNearnessSummary::default(),
            comparisons: Vec::new(),
        }));
    }

    let mut comparisons = Vec::new();
    let mut summary = crate::TopologyNearnessSummary::default();
    for pair in pair_requests {
        let comparison = compare_topology_pair(
            &pair,
            &structure_index,
            &hashkey_index,
            &atom_specs,
            &radius_mode,
            radius_const,
        )?;
        summary.comparisons_total += 1;
        *summary
            .verdict_counts
            .entry(comparison.verdict.clone())
            .or_insert(0) += 1;
        *summary
            .source_counts
            .entry(comparison.source.clone())
            .or_insert(0) += 1;
        *summary
            .detail_counts
            .entry(comparison.detail.clone())
            .or_insert(0) += 1;
        if comparison.left_structure.is_none() || comparison.right_structure.is_none() {
            summary.unresolved_pairs += 1;
        }
        comparisons.push(comparison);
    }

    Ok(Some(crate::TopologyNearnessReport {
        metadata: crate::TopologyNearnessMetadata {
            radius_mode,
            radius_const,
            species_order,
            near_edge_threshold: crate::TOPOLOGY_NEAR_EDGE_THRESHOLD,
            near_coordination_threshold: crate::TOPOLOGY_NEAR_COORDINATION_THRESHOLD,
            sources: crate::TopologyNearnessSources {
                audit_events: raw_dir
                    .join("hashkey_audit_events.csv")
                    .exists()
                    .then(|| "raw/hashkey_audit_events.csv".to_string()),
                top_structures_hashkeys: raw_dir
                    .join("top_structures_hashkeys")
                    .exists()
                    .then(|| "raw/top_structures_hashkeys".to_string()),
                ga_statistics: collect_ga_statistics_paths(raw_dir),
            },
        },
        summary,
        comparisons,
    }))
}

pub fn parse_atoms_file(path: &Path) -> Result<Vec<AtomSpecRecord>> {
    let text = fs::read_to_string(path)
        .with_context(|| format!("failed to read atoms file `{}`", path.display()))?;
    let mut records = Vec::new();
    for line in text.lines().skip(1) {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        let parts = trimmed.split(',').map(str::trim).collect::<Vec<_>>();
        if parts.len() < 5 {
            continue;
        }
        records.push(AtomSpecRecord {
            species: parts[1].to_string(),
            covalent_radius: parts[3].parse().with_context(|| {
                format!("failed to parse covalent radius in `{}`", path.display())
            })?,
            ionic_radius: parts[4]
                .parse()
                .with_context(|| format!("failed to parse ionic radius in `{}`", path.display()))?,
        });
    }
    Ok(records)
}

pub fn index_structure_snapshots(structures_dir: &Path) -> Result<HashMap<String, PathBuf>> {
    let mut index = HashMap::new();
    collect_structure_snapshots(structures_dir, &mut index)?;
    Ok(index)
}

fn collect_structure_snapshots(current: &Path, index: &mut HashMap<String, PathBuf>) -> Result<()> {
    if !current.exists() {
        return Ok(());
    }
    for entry in
        fs::read_dir(current).with_context(|| format!("failed to read `{}`", current.display()))?
    {
        let entry = entry.with_context(|| {
            format!(
                "failed to read directory entry inside `{}`",
                current.display()
            )
        })?;
        let path = entry.path();
        if path.is_dir() {
            collect_structure_snapshots(&path, index)?;
            continue;
        }
        if !is_supported_snapshot_path(&path) {
            continue;
        }
        let Some(stem) = path.file_stem().and_then(|stem| stem.to_str()) else {
            continue;
        };
        let cluster_id = extract_cluster_id(stem);
        index
            .entry(cluster_id.to_string())
            .or_insert_with(|| path.to_path_buf());
    }
    Ok(())
}

fn is_supported_snapshot_path(path: &Path) -> bool {
    matches!(
        path.extension()
            .and_then(|ext| ext.to_str())
            .map(|ext| ext.to_ascii_lowercase())
            .as_deref(),
        Some("xyz" | "car" | "arc" | "cif" | "can")
    )
}

fn extract_cluster_id(stem: &str) -> &str {
    let bytes = stem.as_bytes();
    let mut end = 0usize;
    for (idx, ch) in bytes.iter().enumerate() {
        let is_alnum = ch.is_ascii_alphanumeric();
        if idx == 0 {
            if !is_alnum {
                break;
            }
            end = 1;
            continue;
        }
        let prev_is_alpha = bytes[idx - 1].is_ascii_alphabetic();
        if is_alnum {
            end = idx + 1;
            continue;
        }
        if *ch == b'_' || *ch == b'-' {
            if prev_is_alpha {
                end = idx;
            }
            break;
        }
        break;
    }
    if end == 0 {
        stem
    } else {
        &stem[..end]
    }
}

fn collect_hashkey_index(raw_dir: &Path) -> Result<HashMap<String, String>> {
    let mut index = HashMap::new();
    let top_hashkeys = raw_dir.join("top_structures_hashkeys");
    if top_hashkeys.exists() {
        for (cluster_id, hashkey) in parse_top_structure_hashkeys(&top_hashkeys)? {
            index.insert(cluster_id, hashkey);
        }
    }

    let mut ga_paths = fs::read_dir(raw_dir)
        .with_context(|| format!("failed to read `{}`", raw_dir.display()))?
        .filter_map(|entry| entry.ok().map(|entry| entry.path()))
        .filter(|path| {
            path.file_name()
                .and_then(|name| name.to_str())
                .map(|name| name.starts_with("gaStatistics") && name.ends_with(".csv"))
                .unwrap_or(false)
        })
        .collect::<Vec<_>>();
    ga_paths.sort();
    for path in ga_paths {
        for (cluster_id, hashkey) in parse_ga_statistics_hashkeys(&path)? {
            index.insert(cluster_id, hashkey);
        }
    }
    Ok(index)
}

fn parse_top_structure_hashkeys(path: &Path) -> Result<Vec<(String, String)>> {
    let text =
        fs::read_to_string(path).with_context(|| format!("failed to read `{}`", path.display()))?;
    let mut rows = Vec::new();
    for line in text.lines().skip(1) {
        let parts = line.split_whitespace().collect::<Vec<_>>();
        if parts.len() < 3 {
            continue;
        }
        let cluster_id = parts[1];
        let hashkey = parts[2];
        if cluster_id.eq_ignore_ascii_case("x") || hashkey.eq_ignore_ascii_case("x") {
            continue;
        }
        rows.push((cluster_id.to_string(), hashkey.to_string()));
    }
    Ok(rows)
}

fn parse_ga_statistics_hashkeys(path: &Path) -> Result<Vec<(String, String)>> {
    let text =
        fs::read_to_string(path).with_context(|| format!("failed to read `{}`", path.display()))?;
    let mut rows = Vec::new();
    for line in text.lines().skip(1) {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        let parts = trimmed.split(',').map(str::trim).collect::<Vec<_>>();
        if parts.len() < 3 {
            continue;
        }
        let cluster_id = parts[1];
        let hashkey = parts[2];
        if cluster_id.is_empty()
            || hashkey.is_empty()
            || hashkey.eq_ignore_ascii_case("x")
            || hashkey.eq_ignore_ascii_case("undefined")
        {
            continue;
        }
        rows.push((cluster_id.to_string(), hashkey.to_string()));
    }
    Ok(rows)
}

fn collect_ga_statistics_paths(raw_dir: &Path) -> Vec<String> {
    let mut paths = match fs::read_dir(raw_dir) {
        Ok(entries) => entries
            .filter_map(|entry| entry.ok().map(|entry| entry.path()))
            .filter_map(|path| {
                path.file_name()
                    .and_then(|name| name.to_str())
                    .filter(|name| name.starts_with("gaStatistics") && name.ends_with(".csv"))
                    .map(|name| format!("raw/{name}"))
            })
            .collect::<Vec<_>>(),
        Err(_) => Vec::new(),
    };
    paths.sort();
    paths
}

fn collect_topology_pair_requests(raw_dir: &Path) -> Result<Vec<crate::TopologyPairRequest>> {
    let mut pairs = Vec::new();
    let audit_path = raw_dir.join("hashkey_audit_events.csv");
    if audit_path.exists() {
        for event in parse_topology_audit_events(&audit_path)? {
            if event.event_kind == "duplicate"
                && !event.left_id.is_empty()
                && !event.right_id.is_empty()
            {
                pairs.push(crate::TopologyPairRequest {
                    source: "duplicate_audit".to_string(),
                    detail: event.detail,
                    left_id: event.left_id,
                    right_id: event.right_id,
                });
            }
        }
    }

    let top_hashkeys = raw_dir.join("top_structures_hashkeys");
    if top_hashkeys.exists() {
        let ids = parse_top_structure_hashkeys(&top_hashkeys)?
            .into_iter()
            .map(|(cluster_id, _)| cluster_id)
            .collect::<Vec<_>>();
        for right in 1..ids.len() {
            for left in 0..right {
                pairs.push(crate::TopologyPairRequest {
                    source: "top_structures".to_string(),
                    detail: "top_structure_pair".to_string(),
                    left_id: ids[left].clone(),
                    right_id: ids[right].clone(),
                });
            }
        }
    }

    Ok(pairs)
}

pub fn parse_topology_audit_events(path: &Path) -> Result<Vec<crate::TopologyEventRow>> {
    let text =
        fs::read_to_string(path).with_context(|| format!("failed to read `{}`", path.display()))?;
    let mut rows = Vec::new();
    for line in text.lines().skip(1) {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        let parts = trimmed.split(',').map(str::trim).collect::<Vec<_>>();
        if parts.len() < 5 {
            continue;
        }
        rows.push(crate::TopologyEventRow {
            ga_iter: parts[0].parse().unwrap_or(0),
            event_kind: parts[1].to_string(),
            detail: parts[2].to_string(),
            left_id: parts[3].to_string(),
            right_id: parts[4].to_string(),
        });
    }
    Ok(rows)
}

pub fn compare_topology_pair(
    pair: &crate::TopologyPairRequest,
    structure_index: &HashMap<String, PathBuf>,
    hashkey_index: &HashMap<String, String>,
    atom_specs: &[AtomSpecRecord],
    radius_mode: &str,
    radius_const: f64,
) -> Result<crate::TopologyNearnessComparison> {
    let left_hashkey = hashkey_index.get(&pair.left_id).cloned();
    let right_hashkey = hashkey_index.get(&pair.right_id).cloned();
    let exact_hashkey_match = match (&left_hashkey, &right_hashkey) {
        (Some(left), Some(right)) => Some(left == right),
        _ => None,
    };

    let left_structure = structure_index.get(&pair.left_id).cloned();
    let right_structure = structure_index.get(&pair.right_id).cloned();
    let mut notes = Vec::new();

    let left_summary = match &left_structure {
        Some(path) => Some(build_structure_summary(
            path,
            atom_specs,
            radius_mode,
            radius_const,
        )?),
        None => {
            notes.push(format!("missing structure for {}", pair.left_id));
            None
        }
    };
    let right_summary = match &right_structure {
        Some(path) => Some(build_structure_summary(
            path,
            atom_specs,
            radius_mode,
            radius_const,
        )?),
        None => {
            notes.push(format!("missing structure for {}", pair.right_id));
            None
        }
    };

    let mut edge_diff_by_pair = BTreeMap::new();
    let mut coordination_delta_by_species = BTreeMap::new();
    let mut same_species_counts = None;
    let mut same_dimensionality = None;
    let mut edge_diff_total = None;
    let mut coordination_delta_total = None;
    let left_dimensionality = left_summary.as_ref().map(|summary| summary.dimensionality);
    let right_dimensionality = right_summary.as_ref().map(|summary| summary.dimensionality);
    let verdict = match (&left_summary, &right_summary) {
        (Some(left), Some(right)) => {
            let same_dims = left.dimensionality == right.dimensionality;
            same_dimensionality = Some(same_dims);
            if !same_dims {
                notes.push(format!(
                    "dimensionality differs: {} vs {}",
                    format_dimensionality(left.dimensionality),
                    format_dimensionality(right.dimensionality)
                ));
                "incomparable".to_string()
            } else {
                let same = left.species_counts == right.species_counts;
                same_species_counts = Some(same);
                if same {
                    edge_diff_by_pair =
                        diff_edge_counts(&left.edge_counts_by_pair, &right.edge_counts_by_pair);
                    coordination_delta_by_species = diff_coordination_histograms(
                        &left.coordination_histograms,
                        &right.coordination_histograms,
                    );
                    let edge_total = edge_diff_by_pair.values().copied().sum::<u32>();
                    let coord_total = coordination_delta_by_species
                        .values()
                        .flat_map(|counts| counts.values())
                        .map(|value| value.unsigned_abs())
                        .sum::<u32>();
                    edge_diff_total = Some(edge_total);
                    coordination_delta_total = Some(coord_total);
                    classify_topology_verdict(exact_hashkey_match, edge_total, coord_total)
                        .to_string()
                } else {
                    notes.push("species counts differ".to_string());
                    "incomparable".to_string()
                }
            }
        }
        _ => {
            if exact_hashkey_match == Some(true) {
                "identical".to_string()
            } else {
                "incomparable".to_string()
            }
        }
    };

    Ok(crate::TopologyNearnessComparison {
        source: pair.source.clone(),
        detail: pair.detail.clone(),
        left_id: pair.left_id.clone(),
        right_id: pair.right_id.clone(),
        left_hashkey,
        right_hashkey,
        exact_hashkey_match,
        same_species_counts,
        same_dimensionality,
        left_structure: left_structure
            .as_ref()
            .map(|path| path.display().to_string()),
        right_structure: right_structure
            .as_ref()
            .map(|path| path.display().to_string()),
        left_dimensionality: left_dimensionality
            .map(|value| format_dimensionality(value).to_string()),
        right_dimensionality: right_dimensionality
            .map(|value| format_dimensionality(value).to_string()),
        left_radius: left_summary.as_ref().map(|summary| summary.radius),
        right_radius: right_summary.as_ref().map(|summary| summary.radius),
        edge_diff_total,
        edge_diff_by_pair,
        coordination_delta_total,
        coordination_delta_by_species,
        verdict,
        notes,
    })
}

pub fn build_structure_summary(
    path: &Path,
    atom_specs: &[AtomSpecRecord],
    radius_mode: &str,
    radius_const: f64,
) -> Result<crate::StructureSummary> {
    let snapshot = parse_structure_snapshot(path)?;
    let atoms = snapshot.atoms;
    let species_counts = count_species(&atoms);
    let radius =
        compute_structure_hashkey_radius(&species_counts, atom_specs, radius_mode, radius_const)
            .with_context(|| {
                format!("failed to compute hashkey radius for `{}`", path.display())
            })?;
    let edges = build_structure_edges_with_lattice(
        &atoms,
        snapshot.lattice,
        snapshot.periodic_axes,
        radius,
    );
    let edge_counts_by_pair = summarize_edge_pairs(&atoms, &edges);
    let coordination_histograms = summarize_coordination_histograms(&atoms, &edges);
    Ok(crate::StructureSummary {
        dimensionality: dimensionality_from_axes(snapshot.periodic_axes),
        species_counts,
        edge_counts_by_pair,
        coordination_histograms,
        radius,
    })
}

pub fn parse_candidate_snapshot(path: &Path) -> Result<patina_types::Candidate> {
    let extension = path
        .extension()
        .and_then(|ext| ext.to_str())
        .map(|ext| ext.to_ascii_lowercase());
    if matches!(extension.as_deref(), Some("xyz" | "extxyz")) {
        return candidate_from_extxyz_path(path)
            .with_context(|| format!("failed to parse xyz/extxyz candidate `{}`", path.display()));
    }

    let snapshot = parse_structure_snapshot(path)?;
    let fractional_coords = if let Some(lattice) = snapshot.lattice {
        snapshot
            .atoms
            .iter()
            .map(|atom| {
                patina_search::cartesian_to_fractional(lattice, atom.coords).with_context(|| {
                    format!(
                        "failed to convert Cartesian coordinates to fractional for `{}`",
                        path.display()
                    )
                })
            })
            .collect::<Result<Vec<_>>>()?
            .into_iter()
            .map(|coord| {
                patina_search::normalize_fractional_coordinate_with_axes(
                    coord,
                    snapshot.periodic_axes,
                )
            })
            .collect()
    } else {
        snapshot.atoms.iter().map(|atom| atom.coords).collect()
    };

    Ok(patina_types::Candidate {
        species: snapshot
            .atoms
            .iter()
            .map(|atom| atom.species.clone())
            .collect(),
        fractional_coords,
        lattice: snapshot.lattice,
        periodic_axes: snapshot.periodic_axes,
        label: path
            .file_stem()
            .and_then(|stem| stem.to_str())
            .unwrap_or("candidate")
            .to_string(),
    })
}

#[derive(Debug, Clone)]
struct ParsedStructureSnapshot {
    atoms: Vec<crate::StructureAtom>,
    lattice: Option<[[f64; 3]; 3]>,
    periodic_axes: [bool; 3],
}

fn parse_structure_snapshot(path: &Path) -> Result<ParsedStructureSnapshot> {
    let extension = path
        .extension()
        .and_then(|ext| ext.to_str())
        .map(|ext| ext.to_ascii_lowercase());

    match extension.as_deref() {
        Some("cif") => parse_cif_structure(path),
        Some("car") => parse_car_structure(path),
        Some("arc") => parse_arc_structure(path),
        Some("can") => parse_can_structure(path),
        _ => parse_xyz_like_structure(path),
    }
}

fn parse_xyz_like_structure(path: &Path) -> Result<ParsedStructureSnapshot> {
    let text = fs::read_to_string(path)
        .with_context(|| format!("failed to read structure `{}`", path.display()))?;
    let mut lines = text.lines();
    let _atom_count = lines.next();
    let comment = lines.next().unwrap_or_default();
    let lattice = parse_extxyz_lattice(comment)?;
    let periodic_axes = parse_extxyz_pbc(comment).unwrap_or([
        lattice.is_some(),
        lattice.is_some(),
        lattice.is_some(),
    ]);
    let mut atoms = Vec::new();
    for line in lines {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        let parts = trimmed.split_whitespace().collect::<Vec<_>>();
        if parts.len() < 4 {
            continue;
        }
        atoms.push(crate::StructureAtom {
            species: parts[0].to_string(),
            coords: [
                parts[1].parse().with_context(|| {
                    format!("failed to parse x coordinate in `{}`", path.display())
                })?,
                parts[2].parse().with_context(|| {
                    format!("failed to parse y coordinate in `{}`", path.display())
                })?,
                parts[3].parse().with_context(|| {
                    format!("failed to parse z coordinate in `{}`", path.display())
                })?,
            ],
        });
    }
    Ok(ParsedStructureSnapshot {
        atoms,
        lattice,
        periodic_axes,
    })
}

fn parse_cif_structure(path: &Path) -> Result<ParsedStructureSnapshot> {
    let candidate = candidate_from_cif_path(path).map_err(|err| anyhow!(err))?;
    let lattice = candidate.lattice.with_context(|| {
        format!(
            "cif parser did not produce a lattice for `{}`",
            path.display()
        )
    })?;
    let atoms = candidate
        .species
        .iter()
        .cloned()
        .zip(candidate.fractional_coords.iter().copied())
        .map(|(species, fractional)| crate::StructureAtom {
            species,
            coords: patina_search::fractional_to_cartesian(lattice, fractional),
        })
        .collect();
    Ok(ParsedStructureSnapshot {
        atoms,
        lattice: candidate.lattice,
        periodic_axes: candidate.periodic_axes,
    })
}

fn parse_car_structure(path: &Path) -> Result<ParsedStructureSnapshot> {
    let text = fs::read_to_string(path)
        .with_context(|| format!("failed to read car structure `{}`", path.display()))?;
    let mut atoms = Vec::new();

    for (line_index, line) in text.lines().enumerate() {
        if line_index < 4 {
            continue;
        }
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.eq_ignore_ascii_case("end") {
            continue;
        }

        let parts = trimmed.split_whitespace().collect::<Vec<_>>();
        if parts.len() < 8 {
            continue;
        }
        atoms.push(crate::StructureAtom {
            species: parts[7].to_string(),
            coords: [
                parts[1].parse().with_context(|| {
                    format!("failed to parse x coordinate in `{}`", path.display())
                })?,
                parts[2].parse().with_context(|| {
                    format!("failed to parse y coordinate in `{}`", path.display())
                })?,
                parts[3].parse().with_context(|| {
                    format!("failed to parse z coordinate in `{}`", path.display())
                })?,
            ],
        });
    }

    Ok(ParsedStructureSnapshot {
        atoms,
        lattice: None,
        periodic_axes: [false, false, false],
    })
}

fn parse_arc_structure(path: &Path) -> Result<ParsedStructureSnapshot> {
    let text = fs::read_to_string(path)
        .with_context(|| format!("failed to read arc structure `{}`", path.display()))?;
    let mut lattice = None;
    let mut atoms = Vec::new();
    let mut reading_atoms = false;

    for line in text.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        if trimmed.eq_ignore_ascii_case("end") {
            if reading_atoms {
                break;
            }
            continue;
        }
        if trimmed.starts_with("!BIOSYM")
            || trimmed.eq_ignore_ascii_case("PBC=ON")
            || trimmed.eq_ignore_ascii_case("PBC=OFF")
            || trimmed.starts_with("!DATE")
        {
            continue;
        }
        if trimmed.starts_with("PBC ") {
            let parts = trimmed.split_whitespace().collect::<Vec<_>>();
            if parts.len() < 7 {
                return Err(anyhow!(
                    "ARC cell line in `{}` did not contain six cell parameters",
                    path.display()
                ));
            }
            let params = patina_search::UnitCellParameters {
                a: parts[1].parse().with_context(|| {
                    format!("failed to parse cell length a in `{}`", path.display())
                })?,
                b: parts[2].parse().with_context(|| {
                    format!("failed to parse cell length b in `{}`", path.display())
                })?,
                c: parts[3].parse().with_context(|| {
                    format!("failed to parse cell length c in `{}`", path.display())
                })?,
                alpha_deg: parts[4].parse().with_context(|| {
                    format!("failed to parse cell angle alpha in `{}`", path.display())
                })?,
                beta_deg: parts[5].parse().with_context(|| {
                    format!("failed to parse cell angle beta in `{}`", path.display())
                })?,
                gamma_deg: parts[6].parse().with_context(|| {
                    format!("failed to parse cell angle gamma in `{}`", path.display())
                })?,
            };
            lattice = Some(patina_search::lattice_vectors_from_params(params));
            reading_atoms = true;
            continue;
        }
        if !reading_atoms {
            continue;
        }

        let parts = trimmed.split_whitespace().collect::<Vec<_>>();
        if parts.len() < 4 {
            continue;
        }
        let species = parts
            .get(7)
            .or_else(|| parts.get(6))
            .unwrap_or(&parts[0])
            .to_string();
        atoms.push(crate::StructureAtom {
            species,
            coords: [
                parts[1].parse().with_context(|| {
                    format!("failed to parse x coordinate in `{}`", path.display())
                })?,
                parts[2].parse().with_context(|| {
                    format!("failed to parse y coordinate in `{}`", path.display())
                })?,
                parts[3].parse().with_context(|| {
                    format!("failed to parse z coordinate in `{}`", path.display())
                })?,
            ],
        });
    }

    let lattice = lattice.with_context(|| {
        format!(
            "failed to locate periodic cell parameters in arc structure `{}`",
            path.display()
        )
    })?;
    Ok(ParsedStructureSnapshot {
        atoms,
        lattice: Some(lattice),
        periodic_axes: [true, true, true],
    })
}

fn parse_can_structure(path: &Path) -> Result<ParsedStructureSnapshot> {
    let text = fs::read_to_string(path)
        .with_context(|| format!("failed to read can structure `{}`", path.display()))?;
    let lines = text.lines().map(str::trim).collect::<Vec<_>>();
    if lines.len() < 7 {
        return Err(anyhow!(
            "checkpoint `{}` is too short to be a valid Scott `.can` structure",
            path.display()
        ));
    }

    let atom_count: usize = lines[6]
        .parse()
        .with_context(|| format!("failed to parse atom count in `{}`", path.display()))?;
    let expected_line_count = 7 + atom_count * 7;
    if lines.len() < expected_line_count {
        return Err(anyhow!(
            "checkpoint `{}` ended early: expected at least {} lines for {} atoms, found {}",
            path.display(),
            expected_line_count,
            atom_count,
            lines.len()
        ));
    }

    let mut atoms = Vec::with_capacity(atom_count);
    let mut cursor = 7usize;
    for _ in 0..atom_count {
        let species = lines[cursor].to_string();
        let coords = parse_structure_triplet(lines[cursor + 2], path, "checkpoint Cartesian")?;
        atoms.push(crate::StructureAtom { species, coords });
        cursor += 7;
    }

    Ok(ParsedStructureSnapshot {
        atoms,
        lattice: None,
        periodic_axes: [false, false, false],
    })
}

fn count_species(atoms: &[crate::StructureAtom]) -> BTreeMap<String, usize> {
    let mut counts = BTreeMap::new();
    for atom in atoms {
        *counts.entry(atom.species.clone()).or_insert(0) += 1;
    }
    counts
}

fn to_search_atom_specs(atom_specs: &[AtomSpecRecord]) -> Vec<SearchAtomSpec> {
    atom_specs
        .iter()
        .map(|record| SearchAtomSpec {
            species: record.species.clone(),
            covalent_radius: record.covalent_radius,
            ionic_radius: record.ionic_radius,
        })
        .collect()
}

fn to_search_topology_atoms(atoms: &[crate::StructureAtom]) -> Vec<SearchTopologyAtom> {
    atoms
        .iter()
        .map(|atom| SearchTopologyAtom {
            species: atom.species.clone(),
            coords: atom.coords,
        })
        .collect()
}

pub fn compute_structure_hashkey_radius(
    species_counts: &BTreeMap<String, usize>,
    atom_specs: &[AtomSpecRecord],
    radius_mode: &str,
    radius_const: f64,
) -> Result<f64> {
    let shared_specs = to_search_atom_specs(atom_specs);
    patina_search::compute_structure_hashkey_radius(
        species_counts,
        &shared_specs,
        HashkeyRadiusMode::from_label(radius_mode),
        radius_const,
    )
    .map_err(|err| anyhow!(err))
}

pub fn build_structure_edges_with_lattice(
    atoms: &[crate::StructureAtom],
    lattice: Option<[[f64; 3]; 3]>,
    periodic_axes: [bool; 3],
    radius: f64,
) -> Vec<(usize, usize)> {
    if let Some(lattice) = lattice {
        let mut edges = Vec::new();
        let radius_sq = radius * radius;
        for i in 0..atoms.len() {
            for j in (i + 1)..atoms.len() {
                let dist_sq = patina_search::minimum_image_cartesian_distance_sq_with_axes(
                    atoms[i].coords,
                    atoms[j].coords,
                    Some(lattice),
                    periodic_axes,
                );
                if dist_sq <= radius_sq + 1e-12 {
                    edges.push((i, j));
                }
            }
        }
        return edges;
    }

    let shared_atoms = to_search_topology_atoms(atoms);
    patina_search::build_structure_edges(&shared_atoms, radius)
}

pub fn summarize_edge_pairs(
    atoms: &[crate::StructureAtom],
    edges: &[(usize, usize)],
) -> BTreeMap<String, u32> {
    let shared_atoms = to_search_topology_atoms(atoms);
    patina_search::summarize_edge_pairs(&shared_atoms, edges)
}

pub fn summarize_coordination_histograms(
    atoms: &[crate::StructureAtom],
    edges: &[(usize, usize)],
) -> BTreeMap<String, BTreeMap<u32, u32>> {
    let shared_atoms = to_search_topology_atoms(atoms);
    patina_search::summarize_coordination_histograms(&shared_atoms, edges)
}

pub fn classify_topology_verdict(
    exact_hashkey_match: Option<bool>,
    edge_diff_total: u32,
    coordination_delta_total: u32,
) -> &'static str {
    patina_search::classify_topology_verdict(
        exact_hashkey_match,
        edge_diff_total,
        coordination_delta_total,
        crate::TOPOLOGY_NEAR_EDGE_THRESHOLD,
        crate::TOPOLOGY_NEAR_COORDINATION_THRESHOLD,
    )
}

fn dimensionality_from_axes(periodic_axes: [bool; 3]) -> StructureDimensionality {
    match periodic_axes.into_iter().filter(|enabled| *enabled).count() {
        0 => StructureDimensionality::ZeroD,
        1 => StructureDimensionality::OneD,
        2 => StructureDimensionality::TwoD,
        3 => StructureDimensionality::ThreeD,
        _ => unreachable!("periodic axes always have length three"),
    }
}

fn format_dimensionality(dimensionality: StructureDimensionality) -> &'static str {
    match dimensionality {
        StructureDimensionality::ZeroD => "0D",
        StructureDimensionality::OneD => "1D",
        StructureDimensionality::TwoD => "2D",
        StructureDimensionality::ThreeD => "3D",
    }
}

fn diff_edge_counts(
    left: &BTreeMap<String, u32>,
    right: &BTreeMap<String, u32>,
) -> BTreeMap<String, u32> {
    let mut diff = BTreeMap::new();
    for key in left.keys().chain(right.keys()) {
        let left_count = left.get(key).copied().unwrap_or(0);
        let right_count = right.get(key).copied().unwrap_or(0);
        let delta = left_count.abs_diff(right_count);
        if delta > 0 {
            diff.insert(key.clone(), delta);
        }
    }
    diff
}

fn diff_coordination_histograms(
    left: &BTreeMap<String, BTreeMap<u32, u32>>,
    right: &BTreeMap<String, BTreeMap<u32, u32>>,
) -> BTreeMap<String, BTreeMap<u32, i32>> {
    let mut diff = BTreeMap::new();
    for species in left.keys().chain(right.keys()) {
        let left_hist = left.get(species);
        let right_hist = right.get(species);
        let mut species_diff = BTreeMap::new();
        let keys = left_hist
            .into_iter()
            .flat_map(|hist| hist.keys())
            .chain(right_hist.into_iter().flat_map(|hist| hist.keys()))
            .copied()
            .collect::<Vec<_>>();
        for coordination in keys {
            let left_count = left_hist
                .and_then(|hist| hist.get(&coordination))
                .copied()
                .unwrap_or(0);
            let right_count = right_hist
                .and_then(|hist| hist.get(&coordination))
                .copied()
                .unwrap_or(0);
            let delta = left_count as i32 - right_count as i32;
            if delta != 0 {
                species_diff.insert(coordination, delta);
            }
        }
        if !species_diff.is_empty() {
            diff.insert(species.clone(), species_diff);
        }
    }
    diff
}

fn parse_structure_triplet(line: &str, path: &Path, context: &str) -> Result<[f64; 3]> {
    let parts = line.split_whitespace().collect::<Vec<_>>();
    if parts.len() < 3 {
        return Err(anyhow!(
            "{context} line in `{}` did not contain three coordinates",
            path.display()
        ));
    }
    Ok([
        parts[0]
            .parse()
            .with_context(|| format!("failed to parse x coordinate in `{}`", path.display()))?,
        parts[1]
            .parse()
            .with_context(|| format!("failed to parse y coordinate in `{}`", path.display()))?,
        parts[2]
            .parse()
            .with_context(|| format!("failed to parse z coordinate in `{}`", path.display()))?,
    ])
}

#[cfg(test)]
mod tests {
    use super::*;
    use patina_sci_kernel::codec::cif::parse_cif_symmetry_operation;

    #[test]
    fn parse_extxyz_lattice_reads_three_vectors() {
        let lattice = parse_extxyz_lattice(
            "Lattice=\"1.0, 0.0, 0.0 0.5, 2.0, 0.0 0.0, 0.0, 3.0\" pbc=\"T T T\"",
        )
        .unwrap()
        .unwrap();

        assert_eq!(lattice[0], [1.0, 0.0, 0.0]);
        assert_eq!(lattice[1], [0.5, 2.0, 0.0]);
        assert_eq!(lattice[2], [0.0, 0.0, 3.0]);
    }

    #[test]
    fn parse_extxyz_pbc_reads_dimensionality_flags() {
        let flags = parse_extxyz_pbc("Lattice=\"1 0 0 0 1 0 0 0 1\" pbc=\"T F T\"").unwrap();
        assert_eq!(flags, [true, false, true]);
    }

    #[test]
    fn periodic_edge_builder_uses_minimum_image_distance() {
        let atoms = vec![
            crate::StructureAtom {
                species: "Mg".into(),
                coords: [0.2, 0.0, 0.0],
            },
            crate::StructureAtom {
                species: "O".into(),
                coords: [1.8, 0.0, 0.0],
            },
            crate::StructureAtom {
                species: "O".into(),
                coords: [1.0, 3.0, 3.0],
            },
        ];
        let lattice = Some([[2.0, 0.0, 0.0], [0.0, 6.0, 0.0], [0.0, 0.0, 6.0]]);
        let edges = build_structure_edges_with_lattice(&atoms, lattice, [true, true, true], 0.5);

        assert_eq!(edges, vec![(0, 1)]);
    }

    #[test]
    fn cif_parser_reads_fractional_sites_into_cartesian_snapshot() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("test.cif");
        fs::write(
            &path,
            "data_test
_cell_length_a 4.0
_cell_length_b 5.0
_cell_length_c 6.0
_cell_angle_alpha 90
_cell_angle_beta 90
_cell_angle_gamma 120
loop_
_atom_site_label
_atom_site_fract_x
_atom_site_fract_y
_atom_site_fract_z
Mg1 0.0 0.0 0.0
O1 0.5 0.5 0.5
",
        )
        .unwrap();

        let snapshot = parse_structure_snapshot(&path).unwrap();
        assert!(snapshot.lattice.is_some());
        assert_eq!(snapshot.atoms.len(), 2);
        assert_eq!(snapshot.atoms[0].species, "Mg");
        assert_eq!(snapshot.periodic_axes, [true, true, true]);
    }

    #[test]
    fn cif_symmetry_operation_parser_handles_fractional_offsets() {
        let operation = parse_cif_symmetry_operation("-x+1/2,y+1/4,z").unwrap();
        let transformed = operation.apply([0.1, 0.2, 0.3]);
        assert!((transformed[0] - 0.4).abs() < 1.0e-12);
        assert!((transformed[1] - 0.45).abs() < 1.0e-12);
        assert!((transformed[2] - 0.3).abs() < 1.0e-12);
    }

    #[test]
    fn cif_parser_expands_asymmetric_unit_with_symmetry_operations() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("expanded.cif");
        fs::write(
            &path,
            "data_test
_cell_length_a 4.0
_cell_length_b 4.0
_cell_length_c 4.0
_cell_angle_alpha 90
_cell_angle_beta 90
_cell_angle_gamma 90
loop_
_space_group_symop_operation_xyz
'x, y, z'
'-x, -y, -z'
loop_
_atom_site_type_symbol
_atom_site_fract_x
_atom_site_fract_y
_atom_site_fract_z
Mg 0.1 0.2 0.3
",
        )
        .unwrap();

        let candidate = parse_candidate_snapshot(&path).unwrap();
        assert_eq!(candidate.species, vec!["Mg", "Mg"]);
        assert_eq!(candidate.periodic_axes, [true, true, true]);
        let lattice = candidate.lattice.expect("cif lattice");
        let expected_lattice = [[4.0, 0.0, 0.0], [0.0, 4.0, 0.0], [0.0, 0.0, 4.0]];
        for (row, expected_row) in lattice.iter().zip(expected_lattice) {
            for (value, expected) in row.iter().zip(expected_row) {
                assert!((value - expected).abs() < 1.0e-12);
            }
        }
        assert!((candidate.fractional_coords[0][0] - 0.1).abs() < 1.0e-12);
        assert!((candidate.fractional_coords[0][1] - 0.2).abs() < 1.0e-12);
        assert!((candidate.fractional_coords[0][2] - 0.3).abs() < 1.0e-12);
        assert!((candidate.fractional_coords[1][0] - 0.9).abs() < 1.0e-12);
        assert!((candidate.fractional_coords[1][1] - 0.8).abs() < 1.0e-12);
        assert!((candidate.fractional_coords[1][2] - 0.7).abs() < 1.0e-12);
    }

    #[test]
    fn car_parser_reads_biosym_atom_rows() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("seed.car");
        fs::write(
            &path,
            "!BIOSYM archive 3\nPBC=OFF\nseed\n!DATE2026-03-30\nMg1 0.100000000 0.200000000 0.300000000 XXXX 1 xx Mg 0.000\nO2 1.100000000 1.200000000 1.300000000 XXXX 1 xx O 0.000\nend\nend\n",
        )
        .unwrap();

        let snapshot = parse_structure_snapshot(&path).unwrap();
        assert_eq!(snapshot.lattice, None);
        assert_eq!(snapshot.periodic_axes, [false, false, false]);
        assert_eq!(snapshot.atoms.len(), 2);
        assert_eq!(snapshot.atoms[0].species, "Mg");
        assert_eq!(snapshot.atoms[1].coords, [1.1, 1.2, 1.3]);
    }

    #[test]
    fn arc_parser_reads_periodic_cell_and_atoms() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("seed.arc");
        fs::write(
            &path,
            "!BIOSYM archive 3\nPBC=ON\n                            -12.5000000000\n!DATE\nPBC 4.0000 5.0000 6.0000 90.0000 90.0000 90.0000 (unknown)\nMg 1.0000 2.0000 3.0000 CORE 1 Mg Mg 2.0000\nO 3.0000 4.0000 5.0000 CORE 2 O O -2.0000\nend\nend\n",
        )
        .unwrap();

        let candidate = parse_candidate_snapshot(&path).unwrap();
        assert_eq!(candidate.periodic_axes, [true, true, true]);
        assert_eq!(
            candidate.lattice,
            Some([[4.0, 0.0, 0.0], [0.0, 5.0, 0.0], [0.0, 0.0, 6.0]])
        );
        assert!((candidate.fractional_coords[0][0] - 0.25).abs() < 1.0e-12);
        assert!((candidate.fractional_coords[0][1] - 0.4).abs() < 1.0e-12);
        assert!((candidate.fractional_coords[0][2] - 0.5).abs() < 1.0e-12);
        assert_eq!(candidate.species, vec!["Mg", "O"]);
    }

    #[test]
    fn can_parser_reads_inline_scott_checkpoint_atoms() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("seed.can");
        fs::write(
            &path,
            "A0001\n1 0.0\n-10.0 0.0 0.0\n0.1 0.2 0.3\n0.0 0.0 0.0\n0.0 0.0 0.0\n2\nMg\nc\n0.0 0.0 0.0\n0.0 0.0 0.0\n1\n0.0\n0.0\nO\nc\n1.5 0.0 0.0\n1.5 0.0 0.0\n2\n0.0\n0.0\n",
        )
        .unwrap();

        let candidate = parse_candidate_snapshot(&path).unwrap();
        assert_eq!(candidate.species, vec!["Mg", "O"]);
        assert_eq!(candidate.periodic_axes, [false, false, false]);
        assert_eq!(candidate.lattice, None);
        assert_eq!(candidate.fractional_coords[1], [1.5, 0.0, 0.0]);
    }

    #[test]
    fn parse_candidate_snapshot_preserves_partial_periodicity_from_extxyz() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("wire.extxyz");
        fs::write(
            &path,
            "2
Lattice=\"2 0 0 0 20 0 0 0 20\" pbc=\"T F F\"
Mg 0.1 0.0 0.0
O 1.9 0.0 0.0
",
        )
        .unwrap();

        let candidate = parse_candidate_snapshot(&path).unwrap();
        assert_eq!(candidate.periodic_axes, [true, false, false]);
        assert_eq!(
            candidate.lattice,
            Some([[2.0, 0.0, 0.0], [0.0, 20.0, 0.0], [0.0, 0.0, 20.0]])
        );
        assert!((candidate.fractional_coords[0][0] - 0.05).abs() < 1.0e-12);
        assert!((candidate.fractional_coords[1][0] - 0.95).abs() < 1.0e-12);
    }

    #[test]
    fn periodic_edge_builder_only_wraps_enabled_axes() {
        let atoms = vec![
            crate::StructureAtom {
                species: "Mg".into(),
                coords: [0.2, 0.2, 0.0],
            },
            crate::StructureAtom {
                species: "O".into(),
                coords: [1.8, 1.8, 0.0],
            },
        ];
        let lattice = Some([[2.0, 0.0, 0.0], [0.0, 2.0, 0.0], [0.0, 0.0, 20.0]]);

        let x_only = build_structure_edges_with_lattice(&atoms, lattice, [true, false, false], 0.5);
        let xy = build_structure_edges_with_lattice(&atoms, lattice, [true, true, false], 0.6);

        assert!(x_only.is_empty());
        assert_eq!(xy, vec![(0, 1)]);
    }

    #[test]
    fn build_structure_summary_records_declared_dimensionality() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("wire.extxyz");
        fs::write(
            &path,
            "2
Lattice=\"4 0 0 0 20 0 0 0 20\" pbc=\"T F F\"
Mg 0.0 0.0 0.0
O 2.0 0.0 0.0
",
        )
        .unwrap();

        let summary = build_structure_summary(&path, &sample_atom_specs(), "IR", 0.0).unwrap();
        assert_eq!(summary.dimensionality, StructureDimensionality::OneD);
    }

    #[test]
    fn topology_pair_marks_dimensionality_mismatch_as_incomparable() {
        let dir = tempfile::tempdir().unwrap();
        let left = dir.path().join("cluster.xyz");
        let right = dir.path().join("bulk.extxyz");
        fs::write(&left, "2\ncluster\nMg 0.0 0.0 0.0\nO 1.5 0.0 0.0\n").unwrap();
        fs::write(
            &right,
            "2\nLattice=\"4 0 0 0 4 0 0 0 4\" pbc=\"T T T\"\nMg 0.0 0.0 0.0\nO 2.0 2.0 2.0\n",
        )
        .unwrap();

        let mut structure_index = HashMap::new();
        structure_index.insert("left".to_string(), left);
        structure_index.insert("right".to_string(), right);

        let comparison = compare_topology_pair(
            &crate::TopologyPairRequest {
                source: "test".into(),
                detail: "dimensionality".into(),
                left_id: "left".into(),
                right_id: "right".into(),
            },
            &structure_index,
            &HashMap::new(),
            &sample_atom_specs(),
            "IR",
            0.0,
        )
        .unwrap();

        assert_eq!(comparison.verdict, "incomparable");
        assert_eq!(comparison.same_dimensionality, Some(false));
        assert_eq!(comparison.left_dimensionality.as_deref(), Some("0D"));
        assert_eq!(comparison.right_dimensionality.as_deref(), Some("3D"));
    }

    fn sample_atom_specs() -> Vec<AtomSpecRecord> {
        vec![
            AtomSpecRecord {
                species: "Mg".into(),
                covalent_radius: 1.41,
                ionic_radius: 0.72,
            },
            AtomSpecRecord {
                species: "O".into(),
                covalent_radius: 0.66,
                ionic_radius: 1.26,
            },
        ]
    }
}
