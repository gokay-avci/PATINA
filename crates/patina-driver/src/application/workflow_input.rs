use anyhow::{anyhow, bail, Context, Result};
use patina_types::Candidate;
use serde::Serialize;
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum CandidateInputFormat {
    CandidateJson,
    Xyz,
    ExtXyz,
    Cif,
    Car,
    Arc,
    Can,
}

impl CandidateInputFormat {
    fn from_path(path: &Path) -> Option<Self> {
        match path
            .extension()
            .and_then(|extension| extension.to_str())
            .map(|extension| extension.to_ascii_lowercase())
            .as_deref()
        {
            Some("json") => Some(Self::CandidateJson),
            Some("xyz") => Some(Self::Xyz),
            Some("extxyz") => Some(Self::ExtXyz),
            Some("cif") => Some(Self::Cif),
            Some("car") => Some(Self::Car),
            Some("arc") => Some(Self::Arc),
            Some("can") => Some(Self::Can),
            _ => None,
        }
    }

    fn extension_label(self) -> &'static str {
        match self {
            Self::CandidateJson => ".json",
            Self::Xyz => ".xyz",
            Self::ExtXyz => ".extxyz",
            Self::Cif => ".cif",
            Self::Car => ".car",
            Self::Arc => ".arc",
            Self::Can => ".can",
        }
    }
}

const ALL_CANDIDATE_INPUT_FORMATS: &[CandidateInputFormat] = &[
    CandidateInputFormat::CandidateJson,
    CandidateInputFormat::Xyz,
    CandidateInputFormat::ExtXyz,
    CandidateInputFormat::Cif,
    CandidateInputFormat::Car,
    CandidateInputFormat::Arc,
    CandidateInputFormat::Can,
];

const ALL_CANDIDATE_STAGE_INFERENCE_FORMATS: &[CandidateInputFormat] = &[
    CandidateInputFormat::Xyz,
    CandidateInputFormat::ExtXyz,
    CandidateInputFormat::Cif,
    CandidateInputFormat::Car,
    CandidateInputFormat::Arc,
    CandidateInputFormat::Can,
    CandidateInputFormat::CandidateJson,
];

const CLUSTER_GA_ACCEPTED_FORMATS: &[CandidateInputFormat] = &[
    CandidateInputFormat::CandidateJson,
    CandidateInputFormat::Xyz,
];
const CLUSTER_GA_STAGE_INFERENCE_FORMATS: &[CandidateInputFormat] = &[CandidateInputFormat::Xyz];

#[derive(Debug, Clone, Copy)]
pub(crate) struct SingleCandidateInputContract {
    pub operation: &'static str,
    pub accepted_formats: &'static [CandidateInputFormat],
    pub stage_inference_formats: &'static [CandidateInputFormat],
}

impl SingleCandidateInputContract {
    pub(crate) fn space_group() -> Self {
        Self {
            operation: "structure.space_group",
            accepted_formats: ALL_CANDIDATE_INPUT_FORMATS,
            stage_inference_formats: ALL_CANDIDATE_STAGE_INFERENCE_FORMATS,
        }
    }

    pub(crate) fn cluster_ga_seed() -> Self {
        Self {
            operation: "ga.cluster_seed",
            accepted_formats: CLUSTER_GA_ACCEPTED_FORMATS,
            stage_inference_formats: CLUSTER_GA_STAGE_INFERENCE_FORMATS,
        }
    }

    fn accepted_extensions_label(self) -> String {
        render_format_labels(self.accepted_formats)
    }

    fn inferred_extensions_label(self) -> String {
        render_format_labels(self.stage_inference_formats)
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct CandidateInputSelection {
    pub candidate_json: Option<PathBuf>,
    pub structure_path: Option<PathBuf>,
}

impl CandidateInputSelection {
    pub(crate) fn from_structure_config(
        input: &super::workflow_input_config::StructureInputConfiguration,
    ) -> Self {
        Self {
            candidate_json: input.candidate_json.clone(),
            structure_path: input.structure_path.clone(),
        }
    }

    pub(crate) fn is_empty(&self) -> bool {
        self.candidate_json.is_none() && self.structure_path.is_none()
    }

    pub(crate) fn set_path_by_format(&mut self, path: impl Into<PathBuf>) {
        let path = path.into();
        if matches!(
            CandidateInputFormat::from_path(&path),
            Some(CandidateInputFormat::CandidateJson)
        ) {
            self.candidate_json = Some(path);
            self.structure_path = None;
        } else {
            self.candidate_json = None;
            self.structure_path = Some(path);
        }
    }
}

#[derive(Debug, Clone)]
pub(crate) struct ResolvedCandidateInput {
    pub candidate: Candidate,
    pub source_path: PathBuf,
    pub source_format: CandidateInputFormat,
    pub stage_dir: PathBuf,
}

pub(crate) fn resolve_single_candidate_input(
    contract: SingleCandidateInputContract,
    configured_stage_dir: Option<&Path>,
    selection: &CandidateInputSelection,
) -> Result<ResolvedCandidateInput> {
    let stage_dir = resolve_stage_dir(configured_stage_dir)?;
    let source_path = match (
        selection.candidate_json.as_deref(),
        selection.structure_path.as_deref(),
    ) {
        (Some(_), Some(_)) => {
            bail!(
                "workflow `{}` accepts exactly one input path",
                contract.operation
            )
        }
        (Some(path), None) | (None, Some(path)) => crate::absolutize_path(path)?,
        (None, None) => infer_single_stage_candidate_path(&stage_dir, contract)?,
    };
    let source_format = CandidateInputFormat::from_path(&source_path).ok_or_else(|| {
        anyhow!(
            "workflow `{}` does not support input `{}`; expected one of {}",
            contract.operation,
            source_path.display(),
            contract.accepted_extensions_label()
        )
    })?;
    if !contract.accepted_formats.contains(&source_format) {
        bail!(
            "workflow `{}` does not accept `{}` input `{}`; expected one of {}",
            contract.operation,
            source_format.extension_label(),
            source_path.display(),
            contract.accepted_extensions_label()
        );
    }
    let candidate = load_candidate_input(&source_path, source_format)
        .with_context(|| format!("failed to load `{}`", source_path.display()))?;
    Ok(ResolvedCandidateInput {
        candidate,
        source_path,
        source_format,
        stage_dir,
    })
}

fn resolve_stage_dir(configured_stage_dir: Option<&Path>) -> Result<PathBuf> {
    match configured_stage_dir {
        Some(stage_dir) => crate::absolutize_path(stage_dir),
        None => std::env::current_dir().context("failed to resolve current working directory"),
    }
}

fn infer_single_stage_candidate_path(
    stage_dir: &Path,
    contract: SingleCandidateInputContract,
) -> Result<PathBuf> {
    if !stage_dir.is_dir() {
        bail!(
            "stage directory `{}` does not exist or is not a directory",
            stage_dir.display()
        );
    }

    let candidates = collect_stage_candidate_paths(stage_dir, contract.stage_inference_formats)?;
    match candidates.as_slice() {
        [path] => Ok(path.clone()),
        [] => bail!(
            "stage directory `{}` contains no input for workflow `{}`; expected exactly one of {}",
            stage_dir.display(),
            contract.operation,
            contract.inferred_extensions_label()
        ),
        _ => bail!(
            "stage directory `{}` contains multiple inputs for workflow `{}`; pass one explicitly: {}",
            stage_dir.display(),
            contract.operation,
            render_path_preview(&candidates)
        ),
    }
}

fn collect_stage_candidate_paths(
    stage_dir: &Path,
    allowed_formats: &[CandidateInputFormat],
) -> Result<Vec<PathBuf>> {
    let mut paths = Vec::new();
    for entry in fs::read_dir(stage_dir)
        .with_context(|| format!("failed to read stage directory `{}`", stage_dir.display()))?
    {
        let entry = entry.with_context(|| {
            format!(
                "failed to read an entry in stage directory `{}`",
                stage_dir.display()
            )
        })?;
        let path = entry.path();
        let Some(format) = CandidateInputFormat::from_path(&path) else {
            continue;
        };
        if path.is_file() && allowed_formats.contains(&format) {
            paths.push(path);
        }
    }
    paths.sort();
    Ok(paths)
}

fn load_candidate_input(path: &Path, format: CandidateInputFormat) -> Result<Candidate> {
    match format {
        CandidateInputFormat::CandidateJson => crate::read_candidate_json(path),
        CandidateInputFormat::Xyz
        | CandidateInputFormat::ExtXyz
        | CandidateInputFormat::Cif
        | CandidateInputFormat::Car
        | CandidateInputFormat::Arc
        | CandidateInputFormat::Can => crate::candidate_from_xyz(path),
    }
}

fn render_format_labels(formats: &[CandidateInputFormat]) -> String {
    formats
        .iter()
        .map(|format| format.extension_label())
        .collect::<Vec<_>>()
        .join(", ")
}

fn render_path_preview(paths: &[PathBuf]) -> String {
    let mut labels = paths
        .iter()
        .take(8)
        .map(|path| path.display().to_string())
        .collect::<Vec<_>>();
    if paths.len() > 8 {
        labels.push(format!("... and {} more", paths.len() - 8));
    }
    labels.join(", ")
}

#[cfg(test)]
mod tests {
    use super::{
        resolve_single_candidate_input, CandidateInputFormat, CandidateInputSelection,
        SingleCandidateInputContract,
    };
    use std::fs;

    #[test]
    fn cluster_ga_stage_inference_accepts_exactly_one_xyz() {
        let tempdir = tempfile::tempdir().expect("tempdir");
        let xyz = tempdir.path().join("seed.xyz");
        fs::write(&xyz, "1\nseed\nMg 0.0 0.0 0.0\n").expect("write xyz");

        let resolved = resolve_single_candidate_input(
            SingleCandidateInputContract::cluster_ga_seed(),
            Some(tempdir.path()),
            &CandidateInputSelection::default(),
        )
        .expect("resolve cluster seed");

        assert_eq!(resolved.source_path, xyz);
        assert_eq!(resolved.source_format, CandidateInputFormat::Xyz);
        assert_eq!(resolved.candidate.label, "seed");
    }

    #[test]
    fn cluster_ga_stage_inference_rejects_multiple_xyz_inputs() {
        let tempdir = tempfile::tempdir().expect("tempdir");
        fs::write(tempdir.path().join("a.xyz"), "1\na\nMg 0 0 0\n").expect("write a");
        fs::write(tempdir.path().join("b.xyz"), "1\nb\nMg 0 0 0\n").expect("write b");

        let error = resolve_single_candidate_input(
            SingleCandidateInputContract::cluster_ga_seed(),
            Some(tempdir.path()),
            &CandidateInputSelection::default(),
        )
        .expect_err("multiple xyz inputs should be rejected");

        assert!(error.to_string().contains("multiple inputs"));
    }
}
