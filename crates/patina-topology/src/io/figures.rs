use crate::domain::MotifCandidate;
use crate::fingerprints::{jaccard_similarity, SignatureRecord};
use std::collections::{BTreeMap, BTreeSet};
use std::fmt::Write as _;
use std::fs;
use std::path::{Path, PathBuf};

use crate::domain::TopologyResult;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PreviewArtifacts {
    pub motif_gallery_svg: PathBuf,
    pub signature_summary_svg: PathBuf,
    pub symmetry_summary_svg: PathBuf,
    pub validation_summary_svg: PathBuf,
    pub generator_morphology_matrix_svg: PathBuf,
    pub topology_metrics_svg: PathBuf,
    pub jaccard_graph_dot: PathBuf,
    pub candidate_graphs_dot: PathBuf,
    pub atlas_html: PathBuf,
}

struct BarChartSpec<'a> {
    title: &'a str,
    data: &'a BTreeMap<String, usize>,
    x: f64,
    y: f64,
    w: f64,
    h: f64,
    color: &'a str,
}

pub fn write_preview_bundle(
    out_dir: impl AsRef<Path>,
    candidates: &[MotifCandidate],
    signatures: &[SignatureRecord],
    limit: usize,
    jaccard_threshold: f64,
) -> TopologyResult<PreviewArtifacts> {
    let out_dir = out_dir.as_ref();
    fs::create_dir_all(out_dir)?;
    let motif_gallery_svg = out_dir.join("motif_gallery.svg");
    let signature_summary_svg = out_dir.join("signature_summary.svg");
    let symmetry_summary_svg = out_dir.join("symmetry_summary.svg");
    let validation_summary_svg = out_dir.join("validation_summary.svg");
    let generator_morphology_matrix_svg = out_dir.join("generator_morphology_matrix.svg");
    let topology_metrics_svg = out_dir.join("topology_metrics.svg");
    let jaccard_graph_dot = out_dir.join("jaccard_graph.dot");
    let candidate_graphs_dot = out_dir.join("candidate_graphs.dot");
    let atlas_html = out_dir.join("index.html");

    fs::write(
        &motif_gallery_svg,
        motif_gallery_svg_text(candidates, limit.max(1)),
    )?;
    fs::write(
        &signature_summary_svg,
        signature_summary_svg_text(signatures),
    )?;
    fs::write(&symmetry_summary_svg, symmetry_summary_svg_text(signatures))?;
    fs::write(
        &validation_summary_svg,
        validation_summary_svg_text(candidates),
    )?;
    fs::write(
        &generator_morphology_matrix_svg,
        generator_morphology_matrix_svg_text(signatures),
    )?;
    fs::write(&topology_metrics_svg, topology_metrics_svg_text(signatures))?;
    fs::write(
        &jaccard_graph_dot,
        jaccard_graph_dot_text(signatures, jaccard_threshold),
    )?;
    fs::write(
        &candidate_graphs_dot,
        candidate_graphs_dot_text(candidates, limit.max(1)),
    )?;
    fs::write(
        &atlas_html,
        atlas_html_text(
            candidates,
            signatures,
            &[
                ("Motif gallery", "motif_gallery.svg"),
                ("Signature summary", "signature_summary.svg"),
                ("Symmetry summary", "symmetry_summary.svg"),
                ("Validation summary", "validation_summary.svg"),
                (
                    "Generator morphology matrix",
                    "generator_morphology_matrix.svg",
                ),
                ("Topology metrics", "topology_metrics.svg"),
            ],
            &discover_sidecar_links(out_dir),
        ),
    )?;

    Ok(PreviewArtifacts {
        motif_gallery_svg,
        signature_summary_svg,
        symmetry_summary_svg,
        validation_summary_svg,
        generator_morphology_matrix_svg,
        topology_metrics_svg,
        jaccard_graph_dot,
        candidate_graphs_dot,
        atlas_html,
    })
}

fn motif_gallery_svg_text(candidates: &[MotifCandidate], limit: usize) -> String {
    let shown = candidates.iter().take(limit).collect::<Vec<_>>();
    let cols = 3usize;
    let panel_w = 260.0;
    let panel_h = 230.0;
    let rows = shown.len().div_ceil(cols).max(1);
    let mut svg = svg_header(
        (cols as f64 * panel_w) as usize,
        (rows as f64 * panel_h) as usize,
    );
    for (idx, candidate) in shown.iter().enumerate() {
        let col = idx % cols;
        let row = idx / cols;
        let ox = col as f64 * panel_w;
        let oy = row as f64 * panel_h;
        draw_candidate_panel(&mut svg, candidate, ox, oy, panel_w, panel_h);
    }
    svg.push_str("</svg>\n");
    svg
}

fn draw_candidate_panel(
    svg: &mut String,
    candidate: &MotifCandidate,
    ox: f64,
    oy: f64,
    panel_w: f64,
    panel_h: f64,
) {
    let _ = writeln!(
        svg,
        "<rect x='{ox}' y='{oy}' width='{panel_w}' height='{panel_h}' rx='14' fill='#fbfaf5' stroke='#292521' stroke-width='1'/>"
    );
    let title = format!(
        "{} | {} | {}",
        candidate.id.0,
        candidate.generator.name,
        candidate.composition.total_formula()
    );
    let _ = writeln!(
        svg,
        "<text x='{:.1}' y='{:.1}' font-size='12' font-family='Georgia,serif' fill='#292521'>{}</text>",
        ox + 14.0,
        oy + 22.0,
        escape_xml(&title)
    );

    let projected = project_positions(candidate);
    let (min_x, max_x, min_y, max_y) = bounds(&projected);
    let scale = ((panel_w - 40.0) / (max_x - min_x).max(1.0e-9))
        .min((panel_h - 62.0) / (max_y - min_y).max(1.0e-9));
    let tx = ox + panel_w / 2.0 - scale * 0.5 * (min_x + max_x);
    let ty = oy + panel_h / 2.0 - scale * 0.5 * (min_y + max_y) + 16.0;
    for bond in &candidate.bonds {
        let a = projected[bond.i];
        let b = projected[bond.j];
        let _ = writeln!(
            svg,
            "<line x1='{:.2}' y1='{:.2}' x2='{:.2}' y2='{:.2}' stroke='#6f675f' stroke-width='1.4' opacity='0.72'/>",
            tx + scale * a[0],
            ty - scale * a[1],
            tx + scale * b[0],
            ty - scale * b[1],
        );
    }
    for atom in &candidate.atoms {
        let p = projected[atom.index];
        let color = element_color(atom.element.as_str());
        let _ = writeln!(
            svg,
            "<circle cx='{:.2}' cy='{:.2}' r='5.6' fill='{color}' stroke='#292521' stroke-width='0.8'/>",
            tx + scale * p[0],
            ty - scale * p[1],
        );
    }
}

fn signature_summary_svg_text(signatures: &[SignatureRecord]) -> String {
    let mut morphology = BTreeMap::<String, usize>::new();
    let mut ring_sizes = BTreeMap::<usize, usize>::new();
    for record in signatures {
        *morphology
            .entry(record.signature.geometry.morphology_label.clone())
            .or_insert(0) += 1;
        for (size, count) in &record.signature.rings.ring_size_distribution {
            *ring_sizes.entry(*size).or_insert(0) += count;
        }
    }
    let width = 820usize;
    let height = 420usize;
    let mut svg = svg_header(width, height);
    svg.push_str("<rect x='0' y='0' width='820' height='420' fill='#f4efe4'/>\n");
    svg.push_str("<text x='28' y='38' font-size='24' font-family='Georgia,serif' fill='#292521'>Topology Library Preview</text>\n");
    draw_bar_chart(
        &mut svg,
        BarChartSpec {
            title: "Morphology counts",
            data: &morphology,
            x: 34.0,
            y: 78.0,
            w: 350.0,
            h: 280.0,
            color: "#2f6f73",
        },
    );
    let ring_labels = ring_sizes
        .iter()
        .map(|(size, count)| (format!("ring_{size}"), *count))
        .collect::<BTreeMap<_, _>>();
    draw_bar_chart(
        &mut svg,
        BarChartSpec {
            title: "Ring-size counts",
            data: &ring_labels,
            x: 430.0,
            y: 78.0,
            w: 350.0,
            h: 280.0,
            color: "#b05a2a",
        },
    );
    svg.push_str("</svg>\n");
    svg
}

fn symmetry_summary_svg_text(signatures: &[SignatureRecord]) -> String {
    let mut point_groups = BTreeMap::<String, usize>::new();
    let mut statuses = BTreeMap::<String, usize>::new();
    let mut operation_counts = BTreeMap::<String, usize>::new();
    for record in signatures {
        if let Some(symmetry) = &record.signature.symmetry {
            *statuses.entry(symmetry.status.clone()).or_insert(0) += 1;
            let point_group = symmetry
                .point_group
                .clone()
                .unwrap_or_else(|| "unclassified".to_string());
            *point_groups.entry(point_group).or_insert(0) += 1;
            if let Some(count) = symmetry.operation_count {
                *operation_counts.entry(count.to_string()).or_insert(0) += 1;
            }
        } else {
            *statuses.entry("not_requested".to_string()).or_insert(0) += 1;
            *point_groups.entry("not_requested".to_string()).or_insert(0) += 1;
        }
    }
    let mut svg = svg_header(1180, 440);
    svg.push_str("<rect x='0' y='0' width='1180' height='440' fill='#efe9dc'/>\n");
    svg.push_str("<text x='28' y='38' font-size='24' font-family='Georgia,serif' fill='#292521'>SYVA Point-Symmetry Summary</text>\n");
    draw_bar_chart(
        &mut svg,
        BarChartSpec {
            title: "Point groups",
            data: &point_groups,
            x: 34.0,
            y: 78.0,
            w: 330.0,
            h: 300.0,
            color: "#7c6da8",
        },
    );
    draw_bar_chart(
        &mut svg,
        BarChartSpec {
            title: "Backend status",
            data: &statuses,
            x: 424.0,
            y: 78.0,
            w: 330.0,
            h: 300.0,
            color: "#5f8f68",
        },
    );
    draw_bar_chart(
        &mut svg,
        BarChartSpec {
            title: "Representative operation counts",
            data: &operation_counts,
            x: 814.0,
            y: 78.0,
            w: 330.0,
            h: 300.0,
            color: "#b05a2a",
        },
    );
    svg.push_str("</svg>\n");
    svg
}

fn validation_summary_svg_text(candidates: &[MotifCandidate]) -> String {
    let mut validation = BTreeMap::<String, usize>::new();
    let mut issues = BTreeMap::<String, usize>::new();
    let mut rejection = BTreeMap::<String, usize>::new();
    for candidate in candidates {
        if let Some(report) = candidate.parameters.get("validation_report") {
            let passed = report
                .get("passed")
                .and_then(serde_json::Value::as_bool)
                .unwrap_or(false);
            *validation
                .entry(if passed { "passed" } else { "failed" }.to_string())
                .or_insert(0) += 1;
            if let Some(items) = report.get("issues").and_then(serde_json::Value::as_array) {
                for item in items {
                    if let Some(code) = item.get("code").and_then(serde_json::Value::as_str) {
                        *issues.entry(code.to_string()).or_insert(0) += 1;
                    }
                }
            }
        }
        if let Some(report) = candidate.parameters.get("constrained_random_report") {
            add_json_count(&mut rejection, "degree", report, "rejected_degree");
            add_json_count(
                &mut rejection,
                "chemical_policy",
                report,
                "rejected_chemical_policy",
            );
        }
        if let Some(report) = candidate.parameters.get("topology_mc_report") {
            add_json_count(
                &mut rejection,
                "mc_duplicate_wl",
                report,
                "rejected_duplicate_wl",
            );
            add_json_count(
                &mut rejection,
                "mc_similarity",
                report,
                "rejected_similarity",
            );
        }
    }

    let mut svg = svg_header(1180, 440);
    svg.push_str("<rect x='0' y='0' width='1180' height='440' fill='#f3efe3'/>\n");
    svg.push_str("<text x='28' y='38' font-size='24' font-family='Georgia,serif' fill='#292521'>Validation and Rejection Pressure</text>\n");
    draw_bar_chart(
        &mut svg,
        BarChartSpec {
            title: "Validation status",
            data: &validation,
            x: 34.0,
            y: 78.0,
            w: 330.0,
            h: 300.0,
            color: "#5f8f68",
        },
    );
    draw_bar_chart(
        &mut svg,
        BarChartSpec {
            title: "Validation issue codes",
            data: &issues,
            x: 424.0,
            y: 78.0,
            w: 330.0,
            h: 300.0,
            color: "#b05a2a",
        },
    );
    draw_bar_chart(
        &mut svg,
        BarChartSpec {
            title: "Proposal rejection counts",
            data: &rejection,
            x: 814.0,
            y: 78.0,
            w: 330.0,
            h: 300.0,
            color: "#566d96",
        },
    );
    svg.push_str("</svg>\n");
    svg
}

fn generator_morphology_matrix_svg_text(signatures: &[SignatureRecord]) -> String {
    let mut generators = BTreeSet::<String>::new();
    let mut morphologies = BTreeSet::<String>::new();
    let mut matrix = BTreeMap::<(String, String), usize>::new();
    for record in signatures {
        let generator = record.generator.clone();
        let morphology = record.signature.geometry.morphology_label.clone();
        generators.insert(generator.clone());
        morphologies.insert(morphology.clone());
        *matrix.entry((generator, morphology)).or_insert(0) += 1;
    }
    let generators = generators.into_iter().collect::<Vec<_>>();
    let morphologies = morphologies.into_iter().collect::<Vec<_>>();
    let cell = 58.0;
    let left = 190.0;
    let top = 82.0;
    let width = (left + cell * morphologies.len().max(1) as f64 + 60.0) as usize;
    let height = (top + cell * generators.len().max(1) as f64 + 70.0) as usize;
    let max = matrix.values().copied().max().unwrap_or(1) as f64;
    let mut svg = svg_header(width, height);
    let _ = writeln!(
        svg,
        "<rect x='0' y='0' width='{width}' height='{height}' fill='#f5f0e6'/>"
    );
    svg.push_str("<text x='28' y='38' font-size='24' font-family='Georgia,serif' fill='#292521'>Generator to Morphology Matrix</text>\n");
    for (col, morphology) in morphologies.iter().enumerate() {
        let x = left + col as f64 * cell + cell / 2.0;
        let _ = writeln!(
            svg,
            "<text x='{x:.1}' y='66' transform='rotate(-35 {x:.1} 66)' text-anchor='middle' font-size='12' font-family='Menlo,monospace' fill='#292521'>{}</text>",
            escape_xml(morphology)
        );
    }
    for (row, generator) in generators.iter().enumerate() {
        let y = top + row as f64 * cell;
        let _ = writeln!(
            svg,
            "<text x='{:.1}' y='{:.1}' text-anchor='end' font-size='12' font-family='Menlo,monospace' fill='#292521'>{}</text>",
            left - 12.0,
            y + cell / 2.0 + 4.0,
            escape_xml(generator)
        );
        for (col, morphology) in morphologies.iter().enumerate() {
            let x = left + col as f64 * cell;
            let count = *matrix
                .get(&(generator.clone(), morphology.clone()))
                .unwrap_or(&0);
            let opacity = 0.12 + 0.82 * (count as f64 / max);
            let _ = writeln!(
                svg,
                "<rect x='{x:.1}' y='{y:.1}' width='{:.1}' height='{:.1}' fill='#2f6f73' opacity='{opacity:.3}' stroke='#fffaf0'/>",
                cell - 2.0,
                cell - 2.0
            );
            if count > 0 {
                let _ = writeln!(
                    svg,
                    "<text x='{:.1}' y='{:.1}' text-anchor='middle' font-size='14' font-family='Menlo,monospace' fill='#292521'>{count}</text>",
                    x + cell / 2.0,
                    y + cell / 2.0 + 5.0
                );
            }
        }
    }
    svg.push_str("</svg>\n");
    svg
}

fn topology_metrics_svg_text(signatures: &[SignatureRecord]) -> String {
    let width = 980usize;
    let height = 560usize;
    let mut svg = svg_header(width, height);
    svg.push_str("<rect x='0' y='0' width='980' height='560' fill='#f4efe4'/>\n");
    svg.push_str("<text x='28' y='38' font-size='24' font-family='Georgia,serif' fill='#292521'>Topology Metric Map</text>\n");
    let plot_x = 82.0;
    let plot_y = 78.0;
    let plot_w = 820.0;
    let plot_h = 390.0;
    let max_atoms = signatures
        .iter()
        .map(|record| record.signature.n_atoms)
        .max()
        .unwrap_or(1) as f64;
    let max_cycles = signatures
        .iter()
        .map(|record| record.signature.graph_basic.cycle_rank.max(0) as usize)
        .max()
        .unwrap_or(1) as f64;
    let _ = writeln!(
        svg,
        "<rect x='{plot_x}' y='{plot_y}' width='{plot_w}' height='{plot_h}' rx='12' fill='#fffdf7' stroke='#292521'/>"
    );
    for tick in 0..=5 {
        let x = plot_x + plot_w * tick as f64 / 5.0;
        let y = plot_y + plot_h * tick as f64 / 5.0;
        let _ = writeln!(
            svg,
            "<line x1='{x:.1}' y1='{plot_y}' x2='{x:.1}' y2='{:.1}' stroke='#ded5c9'/>",
            plot_y + plot_h
        );
        let _ = writeln!(
            svg,
            "<line x1='{plot_x}' y1='{y:.1}' x2='{:.1}' y2='{y:.1}' stroke='#ded5c9'/>",
            plot_x + plot_w
        );
    }
    svg.push_str("<text x='420' y='520' font-size='14' font-family='Menlo,monospace' fill='#292521'>n_atoms</text>\n");
    svg.push_str("<text x='24' y='300' transform='rotate(-90 24 300)' font-size='14' font-family='Menlo,monospace' fill='#292521'>cycle_rank</text>\n");
    for record in signatures {
        let atoms = record.signature.n_atoms as f64;
        let cycles = record.signature.graph_basic.cycle_rank.max(0) as f64;
        let edges = record.signature.graph_basic.n_edges as f64;
        let x = plot_x + plot_w * atoms / max_atoms.max(1.0);
        let y = plot_y + plot_h - plot_h * cycles / max_cycles.max(1.0);
        let r = 4.0 + edges.sqrt().min(9.0);
        let color = morphology_color(&record.signature.geometry.morphology_label);
        let label = escape_xml(&format!(
            "{} {} E={} C={}",
            record.candidate_id,
            record.signature.geometry.morphology_label,
            record.signature.graph_basic.n_edges,
            record.signature.graph_basic.cycle_rank
        ));
        let _ = writeln!(
            svg,
            "<circle cx='{x:.2}' cy='{y:.2}' r='{r:.2}' fill='{color}' stroke='#292521' stroke-width='0.7' opacity='0.85'><title>{label}</title></circle>"
        );
    }
    draw_legend(&mut svg, signatures, 730.0, 490.0);
    svg.push_str("</svg>\n");
    svg
}

fn atlas_html_text(
    candidates: &[MotifCandidate],
    signatures: &[SignatureRecord],
    figures: &[(&str, &str)],
    sidecar_links: &[(String, String)],
) -> String {
    let mut generator_counts = BTreeMap::<String, usize>::new();
    let mut morphology_counts = BTreeMap::<String, usize>::new();
    for record in signatures {
        *generator_counts
            .entry(record.generator.clone())
            .or_insert(0) += 1;
        *morphology_counts
            .entry(record.signature.geometry.morphology_label.clone())
            .or_insert(0) += 1;
    }
    let mut html = String::from(
        "<!doctype html><html><head><meta charset='utf-8'><title>PATINA Topology Atlas</title><style>",
    );
    html.push_str("body{margin:0;background:#efe7d6;color:#292521;font-family:Georgia,serif}main{max-width:1180px;margin:0 auto;padding:32px}h1{font-size:42px;margin:0 0 8px}h2{margin-top:34px}.grid{display:grid;grid-template-columns:repeat(auto-fit,minmax(360px,1fr));gap:22px}.card{background:#fffaf0;border:1px solid #292521;border-radius:18px;padding:18px;box-shadow:6px 6px 0 #c9bda9}.metric{font-family:Menlo,monospace;font-size:14px}.figure{width:100%;border-radius:12px;border:1px solid #d4cab8}table{border-collapse:collapse;width:100%;background:#fffaf0}td,th{border:1px solid #c9bda9;padding:8px;text-align:left}code{background:#eadfca;padding:2px 5px;border-radius:5px}</style></head><body><main>");
    let _ = writeln!(
        html,
        "<h1>PATINA Topology Atlas</h1><p class='metric'>{} candidates | {} signatures</p>",
        candidates.len(),
        signatures.len()
    );
    html.push_str("<section class='grid'>");
    html.push_str("<div class='card'><h2>Generator Counts</h2><table><tr><th>Generator</th><th>Count</th></tr>");
    for (key, value) in &generator_counts {
        let _ = writeln!(
            html,
            "<tr><td>{}</td><td>{value}</td></tr>",
            escape_xml(key)
        );
    }
    html.push_str("</table></div><div class='card'><h2>Morphology Counts</h2><table><tr><th>Morphology</th><th>Count</th></tr>");
    for (key, value) in &morphology_counts {
        let _ = writeln!(
            html,
            "<tr><td>{}</td><td>{value}</td></tr>",
            escape_xml(key)
        );
    }
    html.push_str("</table></div></section>");
    html.push_str("<h2>Figures</h2><section class='grid'>");
    for (title, path) in figures {
        let _ = writeln!(
            html,
            "<div class='card'><h3>{}</h3><a href='{path}'><img class='figure' src='{path}' alt='{}'></a></div>",
            escape_xml(title),
            escape_xml(title)
        );
    }
    html.push_str("</section>");
    if !sidecar_links.is_empty() {
        html.push_str("<h2>Sidecar Artifacts</h2><div class='card'><table><tr><th>Artifact</th><th>Path</th></tr>");
        for (title, path) in sidecar_links {
            let _ = writeln!(
                html,
                "<tr><td>{}</td><td><a href='{}'>{}</a></td></tr>",
                escape_xml(title),
                escape_xml(path),
                escape_xml(path)
            );
        }
        html.push_str("</table></div>");
    }
    html.push_str("<h2>Graph Artifacts</h2><div class='card'><p><a href='jaccard_graph.dot'>Jaccard graph DOT</a></p><p><a href='candidate_graphs.dot'>Candidate graph DOT</a></p></div>");
    html.push_str("<h2>Top Candidate Rows</h2><table><tr><th>Candidate</th><th>Generator</th><th>Formula</th><th>Morphology</th><th>Point Group</th><th>Atoms</th><th>Edges</th><th>Cycle Rank</th><th>Fast Hash</th></tr>");
    for record in signatures.iter().take(80) {
        let point_group = record
            .signature
            .symmetry
            .as_ref()
            .and_then(|symmetry| symmetry.point_group.as_deref())
            .unwrap_or("-");
        let _ = writeln!(
            html,
            "<tr><td>{}</td><td>{}</td><td>{}</td><td>{}</td><td>{}</td><td>{}</td><td>{}</td><td>{}</td><td><code>{}</code></td></tr>",
            escape_xml(&record.candidate_id),
            escape_xml(&record.generator),
            escape_xml(&record.signature.formula),
            escape_xml(&record.signature.geometry.morphology_label),
            escape_xml(point_group),
            record.signature.n_atoms,
            record.signature.graph_basic.n_edges,
            record.signature.graph_basic.cycle_rank,
            escape_xml(&record.signature.hashes.fast_hash)
        );
    }
    html.push_str("</table></main></body></html>\n");
    html
}

fn discover_sidecar_links(out_dir: &Path) -> Vec<(String, String)> {
    let mut links = Vec::new();
    let Some(run_dir) = out_dir.parent() else {
        return links;
    };
    let known = [
        (
            run_dir.join("tda").join("rips_tda.jsonl"),
            "TDA Rips JSONL",
            "../tda/rips_tda.jsonl",
        ),
        (
            run_dir.join("tda").join("rips_tda.csv"),
            "TDA Rips CSV",
            "../tda/rips_tda.csv",
        ),
        (
            run_dir.join("tda").join("graph_distance_tda.jsonl"),
            "TDA graph-distance JSONL",
            "../tda/graph_distance_tda.jsonl",
        ),
        (
            run_dir.join("tda").join("graph_distance_tda.csv"),
            "TDA graph-distance CSV",
            "../tda/graph_distance_tda.csv",
        ),
        (
            run_dir.join("tda").join("rips_bottleneck_dim1.dot"),
            "TDA bottleneck graph DOT",
            "../tda/rips_bottleneck_dim1.dot",
        ),
        (
            run_dir.join("xyz_by_morphology"),
            "XYZ folders by morphology",
            "../xyz_by_morphology/",
        ),
        (
            run_dir.join("xyz_by_full_hash"),
            "XYZ folders by full signature hash",
            "../xyz_by_full_hash/",
        ),
        (
            run_dir.join("syva_symmetrized_xyz"),
            "SYVA symmetrized XYZ folders",
            "../syva_symmetrized_xyz/",
        ),
        (
            run_dir
                .join("syva_symmetrized_xyz")
                .join("symmetrization.jsonl"),
            "SYVA symmetrization report",
            "../syva_symmetrized_xyz/symmetrization.jsonl",
        ),
    ];
    for (path, title, href) in known {
        if path.exists() {
            links.push((title.to_string(), href.to_string()));
        }
    }
    let plots = run_dir.join("tda").join("plots");
    if let Ok(entries) = fs::read_dir(&plots) {
        let mut plot_files = entries
            .flatten()
            .filter_map(|entry| {
                let path = entry.path();
                let is_png = path
                    .extension()
                    .and_then(|extension| extension.to_str())
                    .is_some_and(|extension| extension.eq_ignore_ascii_case("png"));
                is_png.then_some(path)
            })
            .collect::<Vec<_>>();
        plot_files.sort();
        for path in plot_files.into_iter().take(8) {
            if let Some(name) = path.file_name().and_then(|name| name.to_str()) {
                links.push((
                    format!("TDA persistence plot {name}"),
                    format!("../tda/plots/{name}"),
                ));
            }
        }
    }
    links
}

fn add_json_count(
    target: &mut BTreeMap<String, usize>,
    label: &str,
    report: &serde_json::Value,
    key: &str,
) {
    let value = report
        .get(key)
        .and_then(serde_json::Value::as_u64)
        .unwrap_or(0) as usize;
    if value > 0 {
        *target.entry(label.to_string()).or_insert(0) += value;
    }
}

fn draw_bar_chart(svg: &mut String, chart: BarChartSpec<'_>) {
    let _ = writeln!(
        svg,
        "<rect x='{x}' y='{y}' width='{w}' height='{h}' rx='14' fill='#fffdf7' stroke='#292521' stroke-width='1'/>",
        x = chart.x,
        y = chart.y,
        w = chart.w,
        h = chart.h
    );
    let _ = writeln!(
        svg,
        "<text x='{:.1}' y='{:.1}' font-size='15' font-family='Georgia,serif' fill='#292521'>{}</text>",
        chart.x + 16.0,
        chart.y + 28.0,
        escape_xml(chart.title)
    );
    let max = chart.data.values().copied().max().unwrap_or(1) as f64;
    let bar_h = 24.0;
    for (idx, (label, value)) in chart.data.iter().enumerate() {
        let yy = chart.y + 55.0 + idx as f64 * 38.0;
        let bw = (chart.w - 150.0) * (*value as f64 / max);
        let _ = writeln!(
            svg,
            "<text x='{:.1}' y='{:.1}' font-size='12' font-family='Menlo,monospace' fill='#292521'>{}</text>",
            chart.x + 16.0,
            yy + 17.0,
            escape_xml(label)
        );
        let _ = writeln!(
            svg,
            "<rect x='{:.1}' y='{:.1}' width='{:.1}' height='{bar_h}' rx='5' fill='{color}' opacity='0.88'/>",
            chart.x + 118.0,
            yy,
            bw,
            color = chart.color
        );
        let _ = writeln!(
            svg,
            "<text x='{:.1}' y='{:.1}' font-size='12' font-family='Menlo,monospace' fill='#292521'>{}</text>",
            chart.x + 126.0 + bw,
            yy + 17.0,
            value
        );
    }
}

fn draw_legend(svg: &mut String, signatures: &[SignatureRecord], x: f64, y: f64) {
    let morphologies = signatures
        .iter()
        .map(|record| record.signature.geometry.morphology_label.clone())
        .collect::<BTreeSet<_>>();
    let mut cursor = x;
    for morphology in morphologies {
        let color = morphology_color(&morphology);
        let _ = writeln!(
            svg,
            "<circle cx='{cursor:.1}' cy='{y:.1}' r='6' fill='{color}' stroke='#292521'/><text x='{:.1}' y='{:.1}' font-size='12' font-family='Menlo,monospace' fill='#292521'>{}</text>",
            cursor + 11.0,
            y + 4.0,
            escape_xml(&morphology)
        );
        cursor += 105.0;
    }
}

fn jaccard_graph_dot_text(signatures: &[SignatureRecord], threshold: f64) -> String {
    let mut out = String::from("graph patina_topology_jaccard {\n  graph [overlap=false, splines=true];\n  node [shape=circle, style=filled, fontname=\"Menlo\"];\n");
    for record in signatures {
        let color = morphology_color(&record.signature.geometry.morphology_label);
        let _ = writeln!(
            out,
            "  \"{}\" [label=\"{}\\n{}\", fillcolor=\"{}\"];",
            record.candidate_id,
            record.candidate_id.replace("candidate_", "c"),
            record.signature.geometry.morphology_label,
            color
        );
    }
    for i in 0..signatures.len() {
        for j in (i + 1)..signatures.len() {
            let score = jaccard_similarity(
                &signatures[i].signature.jaccard_features,
                &signatures[j].signature.jaccard_features,
            );
            if score >= threshold {
                let _ = writeln!(
                    out,
                    "  \"{}\" -- \"{}\" [label=\"{score:.2}\"];",
                    signatures[i].candidate_id, signatures[j].candidate_id
                );
            }
        }
    }
    out.push_str("}\n");
    out
}

fn candidate_graphs_dot_text(candidates: &[MotifCandidate], limit: usize) -> String {
    let mut out = String::from(
        "graph patina_candidate_graphs {\n  node [style=filled, fontname=\"Menlo\"];\n",
    );
    for candidate in candidates.iter().take(limit) {
        let _ = writeln!(
            out,
            "  subgraph \"cluster_{}\" {{",
            candidate.id.0.replace('-', "_")
        );
        let _ = writeln!(
            out,
            "    label=\"{} {}\";",
            candidate.id.0, candidate.generator.name
        );
        for atom in &candidate.atoms {
            let _ = writeln!(
                out,
                "    \"{}:{}\" [label=\"{}{}\", fillcolor=\"{}\"];",
                candidate.id.0,
                atom.index,
                atom.element,
                atom.index,
                element_color(atom.element.as_str())
            );
        }
        for bond in &candidate.bonds {
            let _ = writeln!(
                out,
                "    \"{}:{}\" -- \"{}:{}\";",
                candidate.id.0, bond.i, candidate.id.0, bond.j
            );
        }
        out.push_str("  }\n");
    }
    out.push_str("}\n");
    out
}

fn project_positions(candidate: &MotifCandidate) -> Vec<[f64; 2]> {
    candidate
        .atoms
        .iter()
        .map(|atom| {
            [
                atom.position[0] + 0.22 * atom.position[2],
                atom.position[1] + 0.14 * atom.position[2],
            ]
        })
        .collect()
}

fn bounds(points: &[[f64; 2]]) -> (f64, f64, f64, f64) {
    let mut min_x = f64::INFINITY;
    let mut max_x = f64::NEG_INFINITY;
    let mut min_y = f64::INFINITY;
    let mut max_y = f64::NEG_INFINITY;
    for point in points {
        min_x = min_x.min(point[0]);
        max_x = max_x.max(point[0]);
        min_y = min_y.min(point[1]);
        max_y = max_y.max(point[1]);
    }
    (min_x, max_x, min_y, max_y)
}

fn svg_header(width: usize, height: usize) -> String {
    format!(
        "<svg xmlns='http://www.w3.org/2000/svg' width='{width}' height='{height}' viewBox='0 0 {width} {height}'>\n"
    )
}

fn element_color(element: &str) -> &'static str {
    match element {
        "Ti" => "#6887b8",
        "N" => "#d1b34f",
        "Mg" => "#7ab36a",
        "O" => "#d76f5f",
        "Si" => "#b78d55",
        "Al" => "#9c9c9c",
        "A" => "#5b8c85",
        "B" => "#cc8a4a",
        _ => "#9b7fb7",
    }
}

fn morphology_color(morphology: &str) -> &'static str {
    match morphology {
        "ring" => "#f0c766",
        "barrel" => "#82b9b2",
        "wire" => "#e39a64",
        "cage" => "#95a8d6",
        "multi_shell" => "#b68fc8",
        "compact" => "#b7b09e",
        _ => "#d8d0c1",
    }
}

fn escape_xml(input: &str) -> String {
    input
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

#[allow(dead_code)]
fn unique_elements(candidates: &[MotifCandidate]) -> BTreeSet<String> {
    candidates
        .iter()
        .flat_map(|candidate| candidate.atoms.iter().map(|atom| atom.element.to_string()))
        .collect()
}
