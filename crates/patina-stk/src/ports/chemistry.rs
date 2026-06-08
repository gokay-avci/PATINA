/*!
Chemistry-toolkit port for RDKit-backed or other external chemistry services.
*/

use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use serde::{Deserialize, Serialize};

use crate::building_block::{BuildingBlockRecord, EmbeddedFragmentRecord};
use crate::domain::StkDomainError;
use crate::functional_group::{FunctionalGroupMatch, FunctionalGroupPattern};

pub trait ChemistryToolkitPort {
    fn status(&self) -> Result<ChemistryToolkitStatus, StkDomainError>;
    fn canonicalize_smiles(&self, smiles: &str) -> Result<String, StkDomainError>;
    fn detect_functional_groups(
        &self,
        smiles: &str,
        patterns: &[FunctionalGroupPattern],
    ) -> Result<Vec<FunctionalGroupMatch>, StkDomainError>;
    fn embed_conformer(
        &self,
        smiles: &str,
        seed: u64,
    ) -> Result<EmbeddedFragmentRecord, StkDomainError>;
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ChemistryToolkitStatus {
    pub runtime: String,
    pub python: String,
    pub rdkit: String,
    pub numpy: String,
    pub scipy: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct CanonicalizeSmilesRequest {
    smiles: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct CanonicalizeSmilesResponse {
    canonical_smiles: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct DetectFunctionalGroupsRequest {
    smiles: String,
    patterns: Vec<FunctionalGroupPattern>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct DetectFunctionalGroupsResponse {
    matches: Vec<FunctionalGroupMatch>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct EmbedConformerRequest {
    smiles: String,
    seed: u64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
struct EmbedConformerResponse {
    canonical_smiles: String,
    atom_symbols: Vec<String>,
    atom_positions: Vec<[f64; 3]>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PythonRdkitToolkit {
    python_bin: PathBuf,
    project_dir: PathBuf,
}

impl PythonRdkitToolkit {
    pub fn new(python_bin: impl Into<PathBuf>, project_dir: impl Into<PathBuf>) -> Self {
        Self {
            python_bin: python_bin.into(),
            project_dir: project_dir.into(),
        }
    }

    pub fn workspace_default(root: impl AsRef<Path>) -> Self {
        let root = root.as_ref();
        Self::new(
            root.join("venvs/stk/bin/python"),
            root.join("crates/patina-stk/python"),
        )
    }

    pub fn python_bin(&self) -> &Path {
        &self.python_bin
    }

    pub fn project_dir(&self) -> &Path {
        &self.project_dir
    }

    pub fn prepare_building_block(
        &self,
        label: impl Into<String>,
        smiles: &str,
        patterns: &[FunctionalGroupPattern],
        seed: u64,
    ) -> Result<BuildingBlockRecord, StkDomainError> {
        let canonical_smiles = self.canonicalize_smiles(smiles)?;
        let matches = self.detect_functional_groups(&canonical_smiles, patterns)?;
        let fragment = self.embed_conformer(&canonical_smiles, seed)?;
        BuildingBlockRecord::from_embedded_fragment(label, &fragment, &matches)
    }

    fn invoke_json<TRequest: Serialize, TResponse: for<'de> Deserialize<'de>>(
        &self,
        command: &str,
        request: &TRequest,
    ) -> Result<TResponse, StkDomainError> {
        let module_root = self.project_dir.join("src");
        let request_json =
            serde_json::to_vec(request).map_err(|error| StkDomainError::PythonAdapter {
                reason: format!("failed to serialize request JSON: {error}"),
            })?;

        let mut child = Command::new(&self.python_bin)
            .arg("-m")
            .arg("patina_stk_runtime")
            .arg(command)
            .env("PYTHONPATH", module_root)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|error| StkDomainError::PythonAdapter {
                reason: format!("failed to launch `{}`: {error}", self.python_bin.display()),
            })?;

        if let Some(stdin) = child.stdin.as_mut() {
            stdin
                .write_all(&request_json)
                .and_then(|_| stdin.write_all(b"\n"))
                .map_err(|error| StkDomainError::PythonAdapter {
                    reason: format!("failed to write runtime request: {error}"),
                })?;
        }

        let output = child
            .wait_with_output()
            .map_err(|error| StkDomainError::PythonAdapter {
                reason: format!("failed to wait for runtime output: {error}"),
            })?;

        if !output.status.success() {
            return Err(StkDomainError::PythonAdapter {
                reason: format!(
                    "runtime exited with status {}: {}",
                    output.status,
                    String::from_utf8_lossy(&output.stderr).trim()
                ),
            });
        }

        serde_json::from_slice::<TResponse>(&output.stdout).map_err(|error| {
            StkDomainError::PythonAdapter {
                reason: format!("invalid runtime JSON response: {error}"),
            }
        })
    }
}

impl ChemistryToolkitPort for PythonRdkitToolkit {
    fn status(&self) -> Result<ChemistryToolkitStatus, StkDomainError> {
        self.invoke_json("status", &serde_json::json!({}))
    }

    fn canonicalize_smiles(&self, smiles: &str) -> Result<String, StkDomainError> {
        let response = self.invoke_json::<_, CanonicalizeSmilesResponse>(
            "canonicalize-smiles",
            &CanonicalizeSmilesRequest {
                smiles: smiles.to_string(),
            },
        )?;
        Ok(response.canonical_smiles)
    }

    fn detect_functional_groups(
        &self,
        smiles: &str,
        patterns: &[FunctionalGroupPattern],
    ) -> Result<Vec<FunctionalGroupMatch>, StkDomainError> {
        let response = self.invoke_json::<_, DetectFunctionalGroupsResponse>(
            "detect-functional-groups",
            &DetectFunctionalGroupsRequest {
                smiles: smiles.to_string(),
                patterns: patterns.to_vec(),
            },
        )?;
        Ok(response.matches)
    }

    fn embed_conformer(
        &self,
        smiles: &str,
        seed: u64,
    ) -> Result<EmbeddedFragmentRecord, StkDomainError> {
        let response = self.invoke_json::<_, EmbedConformerResponse>(
            "embed-conformer",
            &EmbedConformerRequest {
                smiles: smiles.to_string(),
                seed,
            },
        )?;
        Ok(EmbeddedFragmentRecord {
            canonical_smiles: response.canonical_smiles,
            atom_symbols: response.atom_symbols,
            atom_positions: response.atom_positions,
        })
    }
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use super::PythonRdkitToolkit;

    #[test]
    fn workspace_default_points_at_venvs_stk() {
        let toolkit = PythonRdkitToolkit::workspace_default("/tmp/patina");
        assert_eq!(
            toolkit.python_bin(),
            Path::new("/tmp/patina/venvs/stk/bin/python")
        );
        assert_eq!(
            toolkit.project_dir(),
            Path::new("/tmp/patina/crates/patina-stk/python")
        );
    }
}
