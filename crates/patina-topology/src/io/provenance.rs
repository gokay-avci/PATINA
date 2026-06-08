use crate::domain::{
    ArtifactRecord, CheckpointChecklist, CheckpointStatus, RunCheckpoint, RunManifest,
    TopologyResult,
};
use std::collections::BTreeMap;
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

pub fn now_unix_seconds() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs())
        .unwrap_or(0)
}

pub fn artifact(
    path: impl AsRef<Path>,
    kind: impl Into<String>,
    role: impl Into<String>,
    record_count: Option<usize>,
) -> ArtifactRecord {
    ArtifactRecord {
        path: path.as_ref().display().to_string(),
        kind: kind.into(),
        role: role.into(),
        record_count,
    }
}

pub fn write_manifest(
    path: impl AsRef<Path>,
    campaign_id: impl Into<String>,
    command: impl Into<String>,
    inputs: BTreeMap<String, String>,
    artifacts: Vec<ArtifactRecord>,
    checklist: CheckpointChecklist,
) -> TopologyResult<()> {
    let cwd = std::env::current_dir()
        .map(|path| path.display().to_string())
        .unwrap_or_else(|_| ".".to_string());
    let manifest = RunManifest {
        schema_version: "patina-topology-run-manifest/v1".to_string(),
        campaign_id: campaign_id.into(),
        command: command.into(),
        cwd,
        created_unix_seconds: now_unix_seconds(),
        inputs,
        artifacts,
        checklist,
    };
    if let Some(parent) = path.as_ref().parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(path, serde_json::to_string_pretty(&manifest)?)?;
    Ok(())
}

pub fn append_checkpoint(
    path: impl AsRef<Path>,
    stage: impl Into<String>,
    status: CheckpointStatus,
    message: impl Into<String>,
    artifacts: Vec<ArtifactRecord>,
) -> TopologyResult<()> {
    if let Some(parent) = path.as_ref().parent() {
        fs::create_dir_all(parent)?;
    }
    let checkpoint = RunCheckpoint {
        timestamp_unix_seconds: now_unix_seconds(),
        stage: stage.into(),
        status,
        message: message.into(),
        artifacts,
    };
    let mut file = OpenOptions::new()
        .create(true)
        .append(true)
        .open(path.as_ref())?;
    serde_json::to_writer(&mut file, &checkpoint)?;
    file.write_all(b"\n")?;
    Ok(())
}
