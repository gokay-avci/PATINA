use std::fs::File;
use std::io::Read;
use std::path::Path;

use anyhow::{Context, Result};
use async_trait::async_trait;
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use surrealdb::engine::local::Db;
use surrealdb::sql::Datetime;
use surrealdb::{RecordId, Surreal};

use crate::application::ports::WorkspaceStorePort;
use crate::database::DatabaseState;
use crate::models::{CatalogSyncReport, WorkspaceCatalog, WorkspaceRunCard};

pub struct SurrealWorkspaceStoreAdapter<'a> {
    database: &'a DatabaseState,
}

impl<'a> SurrealWorkspaceStoreAdapter<'a> {
    pub fn new(database: &'a DatabaseState) -> Self {
        Self { database }
    }
}

#[async_trait]
impl WorkspaceStorePort for SurrealWorkspaceStoreAdapter<'_> {
    async fn sync_workspace_catalog(
        &self,
        workspace_root: &Path,
        catalog: &WorkspaceCatalog,
    ) -> Result<CatalogSyncReport> {
        let db = self.database.client().await?;
        let node_stable_id = self.database.local_node_stable_id();
        let workspace_stable_id =
            stable_id_for_text("workspace", workspace_root.to_string_lossy().as_ref());
        let workspace_revision = stable_revision(catalog)?;
        let now = Datetime::default();

        upsert_content(
            &db,
            "nodes",
            &node_stable_id,
            NodeRecord {
                stable_id: node_stable_id.clone(),
                label: "Local Desktop Node".to_string(),
                claimed_role: "local_first_desktop".to_string(),
                machine_profile: json!({
                    "database_path": self.database.profile(true).path,
                    "os": std::env::consts::OS,
                    "arch": std::env::consts::ARCH,
                }),
                app_version: Some(env!("CARGO_PKG_VERSION").to_string()),
                created_at: now.clone(),
                last_seen_at: Some(now.clone()),
            },
        )
        .await
        .context("upsert local node record")?;

        upsert_content(
            &db,
            "workspaces",
            &workspace_stable_id,
            WorkspaceRecord {
                stable_id: workspace_stable_id.clone(),
                origin_node_id: node_stable_id.clone(),
                revision_token: workspace_revision.clone(),
                slug: slugify_path(workspace_root),
                label: workspace_root
                    .file_name()
                    .map(|value| value.to_string_lossy().into_owned())
                    .unwrap_or_else(|| "workspace".to_string()),
                local_root_path: Some(workspace_root.to_string_lossy().into_owned()),
                created_at: now.clone(),
                updated_at: now.clone(),
            },
        )
        .await
        .context("upsert workspace record")?;

        let workspace_record_id = record_id("workspaces", &workspace_stable_id);
        let mut synced_runs = 0usize;
        let mut synced_run_specs = 0usize;
        let mut synced_structures = 0usize;
        let mut synced_evaluations = 0usize;
        let mut synced_artifacts = 0usize;
        let mut emitted_events = 0usize;

        for run in &catalog.runs {
            let run_stable_id =
                stable_id_for_text("run", format!("{workspace_stable_id}::{}", run.run_name));
            let run_spec_stable_id = stable_id_for_text(
                "run-spec",
                format!("{workspace_stable_id}::spec::{}", run.run_name),
            );
            let run_revision = stable_revision(
                &json!({ "status": run.catalog_status, "manifest": run.manifest_payload }),
            )?;

            let best_structure_record_id = run.best_structure_record.as_ref().map(|_| {
                record_id(
                    "structures",
                    &stable_id_for_text("structure", format!("{run_stable_id}::best")),
                )
            });
            upsert_content(
                &db,
                "run_specs",
                &run_spec_stable_id,
                RunSpecRecord {
                    stable_id: run_spec_stable_id.clone(),
                    origin_node_id: node_stable_id.clone(),
                    revision_token: stable_revision(&run.manifest_payload)?,
                    workspace: Some(workspace_record_id.clone()),
                    workflow_owner: run
                        .workflow_owner
                        .clone()
                        .unwrap_or_else(|| "unknown".to_string()),
                    workflow_scope: run.workflow_scope.clone().unwrap_or_default(),
                    system_label: run.system.clone().unwrap_or_default(),
                    backend_hint: run.backend.clone(),
                    intent: run.manifest_payload.clone().unwrap_or_else(|| {
                        json!({
                            "run_name": run.run_name,
                            "run_spec": run.run_spec,
                        })
                    }),
                    input_structures: Vec::new(),
                    created_at: now.clone(),
                    updated_at: now.clone(),
                },
            )
            .await
            .with_context(|| format!("upsert run_spec for {}", run.run_name))?;
            synced_run_specs += 1;

            if let Some(structure_record) = &run.best_structure_record {
                let structure_stable_id =
                    stable_id_for_text("structure", format!("{run_stable_id}::best"));
                upsert_content(
                    &db,
                    "structures",
                    &structure_stable_id,
                    StructureEntityRecord {
                        stable_id: structure_stable_id.clone(),
                        origin_node_id: node_stable_id.clone(),
                        revision_token: stable_revision(structure_record)?,
                        label: structure_record.label.clone(),
                        structure: serde_json::to_value(structure_record)?,
                        formula: run
                            .best_structure
                            .as_ref()
                            .map(|value| value.formula.clone()),
                        dimensionality: run
                            .best_structure
                            .as_ref()
                            .map(|value| value.dimensionality.clone())
                            .unwrap_or_else(|| "unknown".to_string()),
                        source_kind: Some("ga_generation_state_latest_best_member".to_string()),
                        parent: None,
                        created_at: now.clone(),
                        updated_at: now.clone(),
                    },
                )
                .await
                .with_context(|| format!("upsert structure for {}", run.run_name))?;
                synced_structures += 1;
            }

            if let Some(evaluation_record) = &run.best_evaluation_record {
                let evaluation_stable_id =
                    stable_id_for_text("evaluation", format!("{run_stable_id}::best"));
                upsert_content(
                    &db,
                    "evaluations",
                    &evaluation_stable_id,
                    EvaluationEntityRecord {
                        stable_id: evaluation_stable_id.clone(),
                        origin_node_id: node_stable_id.clone(),
                        revision_token: stable_revision(evaluation_record)?,
                        run: Some(record_id("runs", &run_stable_id)),
                        attempt: None,
                        source_structure: None,
                        relaxed_structure: best_structure_record_id.clone(),
                        summary: serde_json::to_value(evaluation_record)?,
                        energy: Some(evaluation_record.energy),
                        converged: evaluation_record.converged,
                        backend_run_dir: evaluation_record.backend_run_dir.clone(),
                        primary_output_path: evaluation_record.primary_output_path.clone(),
                        created_at: now.clone(),
                    },
                )
                .await
                .with_context(|| format!("upsert evaluation for {}", run.run_name))?;
                synced_evaluations += 1;
            }

            upsert_content(
                &db,
                "runs",
                &run_stable_id,
                RunRecord {
                    stable_id: run_stable_id.clone(),
                    origin_node_id: node_stable_id.clone(),
                    revision_token: run_revision.clone(),
                    workspace: Some(workspace_record_id.clone()),
                    run_spec: Some(record_id("run_specs", &run_spec_stable_id)),
                    status: run.catalog_status.clone(),
                    lane_mode: run.lane_mode.clone(),
                    manifest: run.manifest_payload.clone().unwrap_or_else(|| json!({})),
                    started_at: None,
                    finished_at: None,
                    created_at: now.clone(),
                    updated_at: now.clone(),
                },
            )
            .await
            .with_context(|| format!("upsert run for {}", run.run_name))?;
            synced_runs += 1;

            for (role, rel_path) in artifact_entries(run) {
                let abs_path = Path::new(&run.path).join(&rel_path);
                let artifact_stable_id =
                    stable_id_for_text("artifact", format!("{run_stable_id}::{role}"));
                upsert_content(
                    &db,
                    "artifacts",
                    &artifact_stable_id,
                    ArtifactRecord {
                        stable_id: artifact_stable_id.clone(),
                        origin_node_id: node_stable_id.clone(),
                        revision_token: stable_id_for_text(
                            "artifact-revision",
                            format!("{run_stable_id}::{role}::{rel_path}"),
                        ),
                        owner: Some(record_id("runs", &run_stable_id)),
                        semantic_role: role.clone(),
                        rel_path: Some(rel_path.clone()),
                        abs_path: Some(abs_path.to_string_lossy().into_owned()),
                        media_type: infer_media_type(&abs_path),
                        size_bytes: file_size_bytes(&abs_path),
                        content_hash: file_content_hash(&abs_path)?,
                        shareable: true,
                        manifest: json!({ "declared_path": rel_path }),
                        created_at: now.clone(),
                        updated_at: now.clone(),
                    },
                )
                .await
                .with_context(|| format!("upsert artifact '{role}' for {}", run.run_name))?;
                synced_artifacts += 1;
            }

            upsert_content(
                &db,
                "events",
                &stable_id_for_text(
                    "event",
                    format!("{run_stable_id}::workspace-catalog-synced"),
                ),
                EventRecord {
                    stable_id: stable_id_for_text(
                        "event",
                        format!("{run_stable_id}::workspace-catalog-synced"),
                    ),
                    origin_node_id: node_stable_id.clone(),
                    subject: Some(record_id("runs", &run_stable_id)),
                    event_kind: "workspace_catalog_synced".to_string(),
                    payload: json!({
                        "catalog_status": run.catalog_status,
                        "declared_artifacts": run.artifacts.declared_artifacts,
                    }),
                    occurred_at: now.clone(),
                    immutable: true,
                    created_at: now.clone(),
                },
            )
            .await
            .with_context(|| format!("upsert event for {}", run.run_name))?;
            emitted_events += 1;
        }

        upsert_content(
            &db,
            "sync_ledger",
            &stable_id_for_text(
                "sync",
                format!("{workspace_stable_id}::{workspace_revision}"),
            ),
            SyncLedgerRecord {
                stable_id: stable_id_for_text(
                    "sync",
                    format!("{workspace_stable_id}::{workspace_revision}"),
                ),
                origin_node_id: node_stable_id.clone(),
                peer_node_id: "local-node".to_string(),
                direction: "local_import".to_string(),
                table_name: "workspace_catalog".to_string(),
                record_stable_id: workspace_stable_id.clone(),
                record_revision: Some(workspace_revision),
                artifact_hash: None,
                status: "applied".to_string(),
                detail: json!({
                    "runs": synced_runs,
                    "artifacts": synced_artifacts,
                }),
                synced_at: now.clone(),
            },
        )
        .await
        .context("upsert sync ledger record")?;

        Ok(CatalogSyncReport {
            node_stable_id,
            workspace_stable_id,
            synced_workspaces: 1,
            synced_runs,
            synced_run_specs,
            synced_structures,
            synced_evaluations,
            synced_artifacts,
            emitted_events,
            emitted_sync_receipts: 1,
        })
    }
}

fn artifact_entries(run: &WorkspaceRunCard) -> Vec<(String, String)> {
    run.manifest_payload
        .as_ref()
        .and_then(|manifest| manifest.get("artifacts"))
        .and_then(Value::as_object)
        .map(|entries| {
            entries
                .iter()
                .filter_map(|(key, value)| {
                    value.as_str().map(|path| (key.clone(), path.to_string()))
                })
                .collect()
        })
        .unwrap_or_default()
}

fn stable_id_for_text(prefix: &str, text: impl AsRef<str>) -> String {
    format!(
        "{prefix}-{}",
        blake3::hash(text.as_ref().as_bytes()).to_hex()
    )
}

fn stable_revision<T>(value: &T) -> Result<String>
where
    T: Serialize,
{
    let encoded = serde_json::to_vec(value)?;
    Ok(format!("rev-{}", blake3::hash(&encoded).to_hex()))
}

fn slugify_path(path: &Path) -> String {
    path.file_name()
        .map(|value| value.to_string_lossy().into_owned())
        .unwrap_or_else(|| "workspace".to_string())
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() {
                ch.to_ascii_lowercase()
            } else {
                '-'
            }
        })
        .collect()
}

fn record_id(table: &str, stable_id: &str) -> RecordId {
    RecordId::from((table, stable_id))
}

fn infer_media_type(path: &Path) -> Option<String> {
    match path.extension().and_then(|ext| ext.to_str()) {
        Some("json") => Some("application/json".to_string()),
        Some("csv") => Some("text/csv".to_string()),
        Some("xyz") => Some("chemical/x-xyz".to_string()),
        Some("png") => Some("image/png".to_string()),
        Some("md") => Some("text/markdown".to_string()),
        _ => None,
    }
}

fn file_size_bytes(path: &Path) -> Option<i64> {
    std::fs::metadata(path)
        .ok()
        .filter(|metadata| metadata.is_file())
        .and_then(|metadata| i64::try_from(metadata.len()).ok())
}

fn file_content_hash(path: &Path) -> Result<Option<String>> {
    let metadata = match std::fs::metadata(path) {
        Ok(metadata) if metadata.is_file() => metadata,
        _ => return Ok(None),
    };

    if !metadata.is_file() {
        return Ok(None);
    }

    let mut file = File::open(path)?;
    let mut hasher = blake3::Hasher::new();
    let mut buffer = [0u8; 8192];

    loop {
        let read = file.read(&mut buffer)?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
    }

    Ok(Some(hasher.finalize().to_hex().to_string()))
}

async fn upsert_content<T>(db: &Surreal<Db>, table: &str, stable_id: &str, content: T) -> Result<()>
where
    T: Serialize + DeserializeOwned + Clone + 'static,
{
    let _: Option<T> = db.upsert((table, stable_id)).content(content).await?;
    Ok(())
}

#[derive(Debug, Clone, Deserialize, Serialize)]
struct NodeRecord {
    stable_id: String,
    label: String,
    claimed_role: String,
    machine_profile: Value,
    app_version: Option<String>,
    created_at: Datetime,
    last_seen_at: Option<Datetime>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
struct WorkspaceRecord {
    stable_id: String,
    origin_node_id: String,
    revision_token: String,
    slug: String,
    label: String,
    local_root_path: Option<String>,
    created_at: Datetime,
    updated_at: Datetime,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
struct RunSpecRecord {
    stable_id: String,
    origin_node_id: String,
    revision_token: String,
    workspace: Option<RecordId>,
    workflow_owner: String,
    workflow_scope: String,
    system_label: String,
    backend_hint: Option<String>,
    intent: Value,
    input_structures: Vec<RecordId>,
    created_at: Datetime,
    updated_at: Datetime,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
struct RunRecord {
    stable_id: String,
    origin_node_id: String,
    revision_token: String,
    workspace: Option<RecordId>,
    run_spec: Option<RecordId>,
    status: String,
    lane_mode: Option<String>,
    manifest: Value,
    started_at: Option<Datetime>,
    finished_at: Option<Datetime>,
    created_at: Datetime,
    updated_at: Datetime,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
struct StructureEntityRecord {
    stable_id: String,
    origin_node_id: String,
    revision_token: String,
    label: String,
    structure: Value,
    formula: Option<String>,
    dimensionality: String,
    source_kind: Option<String>,
    parent: Option<RecordId>,
    created_at: Datetime,
    updated_at: Datetime,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
struct EvaluationEntityRecord {
    stable_id: String,
    origin_node_id: String,
    revision_token: String,
    run: Option<RecordId>,
    attempt: Option<RecordId>,
    source_structure: Option<RecordId>,
    relaxed_structure: Option<RecordId>,
    summary: Value,
    energy: Option<f64>,
    converged: bool,
    backend_run_dir: Option<String>,
    primary_output_path: Option<String>,
    created_at: Datetime,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
struct ArtifactRecord {
    stable_id: String,
    origin_node_id: String,
    revision_token: String,
    owner: Option<RecordId>,
    semantic_role: String,
    rel_path: Option<String>,
    abs_path: Option<String>,
    media_type: Option<String>,
    size_bytes: Option<i64>,
    content_hash: Option<String>,
    shareable: bool,
    manifest: Value,
    created_at: Datetime,
    updated_at: Datetime,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
struct EventRecord {
    stable_id: String,
    origin_node_id: String,
    subject: Option<RecordId>,
    event_kind: String,
    payload: Value,
    occurred_at: Datetime,
    immutable: bool,
    created_at: Datetime,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
struct SyncLedgerRecord {
    stable_id: String,
    origin_node_id: String,
    peer_node_id: String,
    direction: String,
    table_name: String,
    record_stable_id: String,
    record_revision: Option<String>,
    artifact_hash: Option<String>,
    status: String,
    detail: Value,
    synced_at: Datetime,
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    use crate::adapters::LocalWorkspaceCatalogAdapter;
    use crate::application::ports::WorkspaceCatalogPort;
    use crate::application::WorkspaceCatalogSyncService;

    #[test]
    fn sync_service_persists_workspace_catalog_into_embedded_store() {
        tauri::async_runtime::block_on(async {
            let temp_dir = tempdir().expect("create tempdir for store sync test");
            let database = DatabaseState::new(temp_dir.path().to_path_buf());
            let workspace_root = Path::new(env!("CARGO_MANIFEST_DIR"))
                .ancestors()
                .nth(2)
                .expect("workspace root");

            let service = WorkspaceCatalogSyncService;
            let catalog_adapter = LocalWorkspaceCatalogAdapter;
            let store_adapter = SurrealWorkspaceStoreAdapter::new(&database);
            let catalog = catalog_adapter
                .load_active_runs(&workspace_root.join("runs").join("active"))
                .expect("load reference workspace catalog");
            let expected_runs = catalog.runs.len();
            let expected_structures = catalog
                .runs
                .iter()
                .filter(|run| run.best_structure_record.is_some())
                .count();
            let expected_evaluations = catalog
                .runs
                .iter()
                .filter(|run| run.best_evaluation_record.is_some())
                .count();

            let report = service
                .sync_reference_workspace_catalog(workspace_root, &catalog_adapter, &store_adapter)
                .await
                .expect("sync workspace catalog into embedded store");

            assert_eq!(report.synced_workspaces, 1);
            assert_eq!(report.synced_runs, expected_runs);
            assert_eq!(report.synced_run_specs, expected_runs);
            assert_eq!(report.synced_structures, expected_structures);
            assert_eq!(report.synced_evaluations, expected_evaluations);

            let db = database.client().await.expect("obtain embedded db client");
            let runs: Vec<RunRecord> = db.select("runs").await.expect("select runs");
            let run_specs: Vec<RunSpecRecord> =
                db.select("run_specs").await.expect("select run specs");
            let structures: Vec<StructureEntityRecord> =
                db.select("structures").await.expect("select structures");
            let evaluations: Vec<EvaluationEntityRecord> =
                db.select("evaluations").await.expect("select evaluations");
            let artifacts: Vec<ArtifactRecord> =
                db.select("artifacts").await.expect("select artifacts");

            assert_eq!(runs.len(), expected_runs);
            assert_eq!(run_specs.len(), expected_runs);
            assert_eq!(structures.len(), expected_structures);
            assert_eq!(evaluations.len(), expected_evaluations);
            assert!(artifacts.len() >= expected_runs);
        });
    }
}
