use anyhow::{anyhow, bail, Context, Result};
use patina_types::Candidate;
use std::fs;
use std::io::Write as _;
use std::path::{Path, PathBuf};
use std::process::Command;

use crate::{build_dreadnaut_graph_text, compute_hashkey_radius, AtomSpec};

pub fn bundled_dreadnaut_path() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("nauty25r9")
        .join("dreadnaut")
}

pub fn resolve_dreadnaut_path(explicit: Option<&Path>) -> Result<PathBuf> {
    if let Some(path) = explicit {
        if path.is_file() {
            return Ok(path.to_path_buf());
        }
        bail!("dreadnaut binary `{}` does not exist", path.display());
    }

    if let Ok(env_path) = std::env::var("PATINA_DREADNAUT_PATH") {
        let path = PathBuf::from(env_path);
        if path.is_file() {
            return Ok(path);
        }
    }

    let bundled = bundled_dreadnaut_path();
    if bundled.is_file() {
        return Ok(bundled);
    }

    bail!(
        "could not locate dreadnaut; tried `--dreadnaut`, `PATINA_DREADNAUT_PATH`, and bundled `{}`",
        bundled.display()
    )
}

pub fn export_dreadnaut_graph(
    candidate: &Candidate,
    atom_specs: &[AtomSpec],
    radius_mode: &str,
    radius_const: f64,
    output_path: &Path,
) -> Result<()> {
    let radius = compute_hashkey_radius(candidate, atom_specs, radius_mode, radius_const)?;
    let graph_text = build_dreadnaut_graph_text(candidate, radius, atom_specs);
    fs::write(output_path, graph_text)
        .with_context(|| format!("failed to write graph `{}`", output_path.display()))?;
    Ok(())
}

pub fn canonical_hashkey_from_graph_text(
    dreadnaut_path: &Path,
    graph_text: &str,
) -> Result<String> {
    let mut child = Command::new(dreadnaut_path)
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .with_context(|| format!("failed to execute `{}`", dreadnaut_path.display()))?;
    let mut stdin = child
        .stdin
        .take()
        .ok_or_else(|| anyhow!("failed to open stdin for `{}`", dreadnaut_path.display()))?;
    stdin.write_all(graph_text.as_bytes()).with_context(|| {
        format!(
            "failed to write graph text to `{}`",
            dreadnaut_path.display()
        )
    })?;
    drop(stdin);
    let output = child
        .wait_with_output()
        .with_context(|| format!("failed to wait for `{}`", dreadnaut_path.display()))?;
    if !output.status.success() {
        bail!(
            "dreadnaut failed with exit code {}: {}",
            output.status,
            String::from_utf8_lossy(&output.stderr).trim()
        );
    }
    parse_dreadnaut_stdout(&String::from_utf8_lossy(&output.stdout), graph_text)
}

pub fn canonical_hashkey_from_graph_file(
    dreadnaut_path: &Path,
    graph_path: &Path,
) -> Result<String> {
    let graph_text = fs::read_to_string(graph_path)
        .with_context(|| format!("failed to read graph `{}`", graph_path.display()))?;
    canonical_hashkey_from_graph_text(dreadnaut_path, &graph_text)
}

pub fn canonical_hashkey_via_legacy_wrapper(
    wrapper_path: &Path,
    graph_path: &Path,
) -> Result<String> {
    let output = Command::new("python3")
        .arg(wrapper_path)
        .arg("-e")
        .arg(graph_path)
        .output()
        .with_context(|| format!("failed to execute `{}`", wrapper_path.display()))?;
    if !output.status.success() {
        bail!(
            "legacy wrapper failed with exit code {}: {}",
            output.status,
            String::from_utf8_lossy(&output.stderr).trim()
        );
    }
    let hashkey = String::from_utf8_lossy(&output.stdout).trim().to_string();
    if hashkey.is_empty() {
        bail!("legacy wrapper completed but returned an empty hashkey");
    }
    Ok(hashkey)
}

pub(crate) fn parse_dreadnaut_stdout(stdout: &str, graph_text: &str) -> Result<String> {
    if stdout.is_empty() && !graph_text.is_empty() {
        bail!("dreadnaut returned empty stdout");
    }
    for line in stdout.lines().rev() {
        let trimmed = line.trim();
        if trimmed.starts_with('[') && trimmed.ends_with(']') {
            return Ok(trimmed.replace(' ', "_").replace(['[', ']'], ""));
        }
    }
    Err(anyhow!(
        "dreadnaut completed but no canonical hashkey line was found in stdout"
    ))
}

#[cfg(test)]
mod tests {
    use super::parse_dreadnaut_stdout;

    #[test]
    fn stdout_parser_extracts_last_bracketed_hashkey() {
        let hashkey = parse_dreadnaut_stdout("noise\n[1 2 3]\n", "x").expect("hashkey");
        assert_eq!(hashkey, "1_2_3");
    }
}
