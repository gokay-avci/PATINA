use std::path::PathBuf;

use surrealdb::engine::local::{Db, RocksDb};
use surrealdb::Surreal;
use tauri::async_runtime::Mutex;
use thiserror::Error;

use crate::models::DatabaseProfile;

pub struct DatabaseState {
    client: Mutex<Option<Surreal<Db>>>,
    path: PathBuf,
    namespace: String,
    database: String,
}

impl DatabaseState {
    pub fn new(app_data_dir: PathBuf) -> Self {
        Self {
            client: Mutex::new(None),
            path: app_data_dir.join("surrealdb"),
            namespace: "patina".to_string(),
            database: "desktop".to_string(),
        }
    }

    pub fn profile(&self, connected: bool) -> DatabaseProfile {
        DatabaseProfile {
            engine: "surrealdb+rocksdb".to_string(),
            namespace: self.namespace.clone(),
            database: self.database.clone(),
            path: self.path.to_string_lossy().into_owned(),
            connected,
        }
    }

    pub fn local_node_stable_id(&self) -> String {
        let hash = blake3::hash(self.path.to_string_lossy().as_bytes());
        format!("node-{}", hash.to_hex())
    }

    pub async fn ensure_connected(&self) -> Result<DatabaseProfile, DatabaseError> {
        let mut guard = self.client.lock().await;
        if guard.is_some() {
            return Ok(self.profile(true));
        }

        std::fs::create_dir_all(&self.path).map_err(|source| DatabaseError::CreateDir {
            path: self.path.clone(),
            source,
        })?;

        let endpoint = self.path.to_string_lossy().into_owned();
        let db = Surreal::new::<RocksDb>(endpoint).await?;
        db.use_ns(self.namespace.as_str())
            .use_db(self.database.as_str())
            .await?;
        db.query(schema_bootstrap_query()).await?;

        *guard = Some(db);
        Ok(self.profile(true))
    }

    pub async fn client(&self) -> Result<Surreal<Db>, DatabaseError> {
        self.ensure_connected().await?;
        let guard = self.client.lock().await;
        guard
            .as_ref()
            .cloned()
            .ok_or(DatabaseError::ClientUnavailable)
    }
}

fn schema_bootstrap_query() -> &'static str {
    r#"
    DEFINE TABLE IF NOT EXISTS nodes SCHEMAFULL;
    DEFINE FIELD IF NOT EXISTS stable_id ON TABLE nodes TYPE string;
    DEFINE FIELD IF NOT EXISTS label ON TABLE nodes TYPE string;
    DEFINE FIELD IF NOT EXISTS claimed_role ON TABLE nodes TYPE string;
    DEFINE FIELD IF NOT EXISTS machine_profile ON TABLE nodes TYPE object FLEXIBLE;
    DEFINE FIELD IF NOT EXISTS app_version ON TABLE nodes TYPE option<string>;
    DEFINE FIELD IF NOT EXISTS created_at ON TABLE nodes TYPE datetime;
    DEFINE FIELD IF NOT EXISTS last_seen_at ON TABLE nodes TYPE option<datetime>;
    DEFINE INDEX IF NOT EXISTS nodes_stable_id ON TABLE nodes FIELDS stable_id UNIQUE;

    DEFINE TABLE IF NOT EXISTS workspaces SCHEMAFULL;
    DEFINE FIELD IF NOT EXISTS stable_id ON TABLE workspaces TYPE string;
    DEFINE FIELD IF NOT EXISTS origin_node_id ON TABLE workspaces TYPE string;
    DEFINE FIELD IF NOT EXISTS revision_token ON TABLE workspaces TYPE string;
    DEFINE FIELD IF NOT EXISTS slug ON TABLE workspaces TYPE string;
    DEFINE FIELD IF NOT EXISTS label ON TABLE workspaces TYPE string;
    DEFINE FIELD IF NOT EXISTS local_root_path ON TABLE workspaces TYPE option<string>;
    DEFINE FIELD IF NOT EXISTS created_at ON TABLE workspaces TYPE datetime;
    DEFINE FIELD IF NOT EXISTS updated_at ON TABLE workspaces TYPE datetime;
    DEFINE INDEX IF NOT EXISTS workspaces_stable_id ON TABLE workspaces FIELDS stable_id UNIQUE;

    DEFINE TABLE IF NOT EXISTS structures SCHEMAFULL;
    DEFINE FIELD IF NOT EXISTS stable_id ON TABLE structures TYPE string;
    DEFINE FIELD IF NOT EXISTS origin_node_id ON TABLE structures TYPE string;
    DEFINE FIELD IF NOT EXISTS revision_token ON TABLE structures TYPE string;
    DEFINE FIELD IF NOT EXISTS label ON TABLE structures TYPE string;
    DEFINE FIELD IF NOT EXISTS structure ON TABLE structures TYPE object FLEXIBLE;
    DEFINE FIELD IF NOT EXISTS formula ON TABLE structures TYPE option<string>;
    DEFINE FIELD IF NOT EXISTS dimensionality ON TABLE structures TYPE string;
    DEFINE FIELD IF NOT EXISTS source_kind ON TABLE structures TYPE option<string>;
    DEFINE FIELD IF NOT EXISTS parent ON TABLE structures TYPE option<record<structures>>;
    DEFINE FIELD IF NOT EXISTS created_at ON TABLE structures TYPE datetime;
    DEFINE FIELD IF NOT EXISTS updated_at ON TABLE structures TYPE datetime;
    DEFINE INDEX IF NOT EXISTS structures_stable_id ON TABLE structures FIELDS stable_id UNIQUE;

    DEFINE TABLE IF NOT EXISTS surfaces SCHEMAFULL;
    DEFINE FIELD IF NOT EXISTS stable_id ON TABLE surfaces TYPE string;
    DEFINE FIELD IF NOT EXISTS origin_node_id ON TABLE surfaces TYPE string;
    DEFINE FIELD IF NOT EXISTS revision_token ON TABLE surfaces TYPE string;
    DEFINE FIELD IF NOT EXISTS label ON TABLE surfaces TYPE string;
    DEFINE FIELD IF NOT EXISTS surface_definition ON TABLE surfaces TYPE object FLEXIBLE;
    DEFINE FIELD IF NOT EXISTS parent_structure ON TABLE surfaces TYPE option<record<structures>>;
    DEFINE FIELD IF NOT EXISTS created_at ON TABLE surfaces TYPE datetime;
    DEFINE FIELD IF NOT EXISTS updated_at ON TABLE surfaces TYPE datetime;
    DEFINE INDEX IF NOT EXISTS surfaces_stable_id ON TABLE surfaces FIELDS stable_id UNIQUE;

    DEFINE TABLE IF NOT EXISTS run_specs SCHEMAFULL;
    DEFINE FIELD IF NOT EXISTS stable_id ON TABLE run_specs TYPE string;
    DEFINE FIELD IF NOT EXISTS origin_node_id ON TABLE run_specs TYPE string;
    DEFINE FIELD IF NOT EXISTS revision_token ON TABLE run_specs TYPE string;
    DEFINE FIELD IF NOT EXISTS workspace ON TABLE run_specs TYPE option<record<workspaces>>;
    DEFINE FIELD IF NOT EXISTS workflow_owner ON TABLE run_specs TYPE string;
    DEFINE FIELD IF NOT EXISTS workflow_scope ON TABLE run_specs TYPE string;
    DEFINE FIELD IF NOT EXISTS system_label ON TABLE run_specs TYPE string;
    DEFINE FIELD IF NOT EXISTS backend_hint ON TABLE run_specs TYPE option<string>;
    DEFINE FIELD IF NOT EXISTS intent ON TABLE run_specs TYPE object FLEXIBLE;
    DEFINE FIELD IF NOT EXISTS input_structures ON TABLE run_specs TYPE array<record<structures>>;
    DEFINE FIELD IF NOT EXISTS created_at ON TABLE run_specs TYPE datetime;
    DEFINE FIELD IF NOT EXISTS updated_at ON TABLE run_specs TYPE datetime;
    DEFINE INDEX IF NOT EXISTS run_specs_stable_id ON TABLE run_specs FIELDS stable_id UNIQUE;

    DEFINE TABLE IF NOT EXISTS runs SCHEMAFULL;
    DEFINE FIELD IF NOT EXISTS stable_id ON TABLE runs TYPE string;
    DEFINE FIELD IF NOT EXISTS origin_node_id ON TABLE runs TYPE string;
    DEFINE FIELD IF NOT EXISTS revision_token ON TABLE runs TYPE string;
    DEFINE FIELD IF NOT EXISTS workspace ON TABLE runs TYPE option<record<workspaces>>;
    DEFINE FIELD IF NOT EXISTS run_spec ON TABLE runs TYPE option<record<run_specs>>;
    DEFINE FIELD IF NOT EXISTS status ON TABLE runs TYPE string;
    DEFINE FIELD IF NOT EXISTS lane_mode ON TABLE runs TYPE option<string>;
    DEFINE FIELD IF NOT EXISTS manifest ON TABLE runs TYPE object FLEXIBLE;
    DEFINE FIELD IF NOT EXISTS started_at ON TABLE runs TYPE option<datetime>;
    DEFINE FIELD IF NOT EXISTS finished_at ON TABLE runs TYPE option<datetime>;
    DEFINE FIELD IF NOT EXISTS created_at ON TABLE runs TYPE datetime;
    DEFINE FIELD IF NOT EXISTS updated_at ON TABLE runs TYPE datetime;
    DEFINE INDEX IF NOT EXISTS runs_stable_id ON TABLE runs FIELDS stable_id UNIQUE;
    DEFINE INDEX IF NOT EXISTS runs_status ON TABLE runs FIELDS status;

    DEFINE TABLE IF NOT EXISTS execution_targets SCHEMAFULL;
    DEFINE FIELD IF NOT EXISTS stable_id ON TABLE execution_targets TYPE string;
    DEFINE FIELD IF NOT EXISTS origin_node_id ON TABLE execution_targets TYPE string;
    DEFINE FIELD IF NOT EXISTS revision_token ON TABLE execution_targets TYPE string;
    DEFINE FIELD IF NOT EXISTS kind ON TABLE execution_targets TYPE string;
    DEFINE FIELD IF NOT EXISTS label ON TABLE execution_targets TYPE string;
    DEFINE FIELD IF NOT EXISTS connection ON TABLE execution_targets TYPE object FLEXIBLE;
    DEFINE FIELD IF NOT EXISTS capabilities ON TABLE execution_targets TYPE object FLEXIBLE;
    DEFINE FIELD IF NOT EXISTS created_at ON TABLE execution_targets TYPE datetime;
    DEFINE FIELD IF NOT EXISTS updated_at ON TABLE execution_targets TYPE datetime;
    DEFINE INDEX IF NOT EXISTS execution_targets_stable_id ON TABLE execution_targets FIELDS stable_id UNIQUE;

    DEFINE TABLE IF NOT EXISTS run_attempts SCHEMAFULL;
    DEFINE FIELD IF NOT EXISTS stable_id ON TABLE run_attempts TYPE string;
    DEFINE FIELD IF NOT EXISTS origin_node_id ON TABLE run_attempts TYPE string;
    DEFINE FIELD IF NOT EXISTS revision_token ON TABLE run_attempts TYPE string;
    DEFINE FIELD IF NOT EXISTS run ON TABLE run_attempts TYPE record<runs>;
    DEFINE FIELD IF NOT EXISTS execution_target ON TABLE run_attempts TYPE option<record<execution_targets>>;
    DEFINE FIELD IF NOT EXISTS attempt_index ON TABLE run_attempts TYPE int;
    DEFINE FIELD IF NOT EXISTS status ON TABLE run_attempts TYPE string;
    DEFINE FIELD IF NOT EXISTS backend ON TABLE run_attempts TYPE string;
    DEFINE FIELD IF NOT EXISTS workdir ON TABLE run_attempts TYPE option<string>;
    DEFINE FIELD IF NOT EXISTS summary ON TABLE run_attempts TYPE object FLEXIBLE;
    DEFINE FIELD IF NOT EXISTS started_at ON TABLE run_attempts TYPE option<datetime>;
    DEFINE FIELD IF NOT EXISTS finished_at ON TABLE run_attempts TYPE option<datetime>;
    DEFINE FIELD IF NOT EXISTS created_at ON TABLE run_attempts TYPE datetime;
    DEFINE FIELD IF NOT EXISTS updated_at ON TABLE run_attempts TYPE datetime;
    DEFINE INDEX IF NOT EXISTS run_attempts_stable_id ON TABLE run_attempts FIELDS stable_id UNIQUE;

    DEFINE TABLE IF NOT EXISTS evaluations SCHEMAFULL;
    DEFINE FIELD IF NOT EXISTS stable_id ON TABLE evaluations TYPE string;
    DEFINE FIELD IF NOT EXISTS origin_node_id ON TABLE evaluations TYPE string;
    DEFINE FIELD IF NOT EXISTS revision_token ON TABLE evaluations TYPE string;
    DEFINE FIELD IF NOT EXISTS run ON TABLE evaluations TYPE option<record<runs>>;
    DEFINE FIELD IF NOT EXISTS attempt ON TABLE evaluations TYPE option<record<run_attempts>>;
    DEFINE FIELD IF NOT EXISTS source_structure ON TABLE evaluations TYPE option<record<structures>>;
    DEFINE FIELD IF NOT EXISTS relaxed_structure ON TABLE evaluations TYPE option<record<structures>>;
    DEFINE FIELD IF NOT EXISTS summary ON TABLE evaluations TYPE object FLEXIBLE;
    DEFINE FIELD IF NOT EXISTS energy ON TABLE evaluations TYPE option<number>;
    DEFINE FIELD IF NOT EXISTS converged ON TABLE evaluations TYPE bool;
    DEFINE FIELD IF NOT EXISTS backend_run_dir ON TABLE evaluations TYPE option<string>;
    DEFINE FIELD IF NOT EXISTS primary_output_path ON TABLE evaluations TYPE option<string>;
    DEFINE FIELD IF NOT EXISTS created_at ON TABLE evaluations TYPE datetime;
    DEFINE INDEX IF NOT EXISTS evaluations_stable_id ON TABLE evaluations FIELDS stable_id UNIQUE;
    DEFINE INDEX IF NOT EXISTS evaluations_energy ON TABLE evaluations FIELDS energy;

    DEFINE TABLE IF NOT EXISTS artifacts SCHEMAFULL;
    DEFINE FIELD IF NOT EXISTS stable_id ON TABLE artifacts TYPE string;
    DEFINE FIELD IF NOT EXISTS origin_node_id ON TABLE artifacts TYPE string;
    DEFINE FIELD IF NOT EXISTS revision_token ON TABLE artifacts TYPE string;
    DEFINE FIELD IF NOT EXISTS owner ON TABLE artifacts TYPE option<record>;
    DEFINE FIELD IF NOT EXISTS semantic_role ON TABLE artifacts TYPE string;
    DEFINE FIELD IF NOT EXISTS rel_path ON TABLE artifacts TYPE option<string>;
    DEFINE FIELD IF NOT EXISTS abs_path ON TABLE artifacts TYPE option<string>;
    DEFINE FIELD IF NOT EXISTS media_type ON TABLE artifacts TYPE option<string>;
    DEFINE FIELD IF NOT EXISTS size_bytes ON TABLE artifacts TYPE option<int>;
    DEFINE FIELD IF NOT EXISTS content_hash ON TABLE artifacts TYPE option<string>;
    DEFINE FIELD IF NOT EXISTS shareable ON TABLE artifacts TYPE bool;
    DEFINE FIELD IF NOT EXISTS manifest ON TABLE artifacts TYPE object FLEXIBLE;
    DEFINE FIELD IF NOT EXISTS created_at ON TABLE artifacts TYPE datetime;
    DEFINE FIELD IF NOT EXISTS updated_at ON TABLE artifacts TYPE datetime;
    DEFINE INDEX IF NOT EXISTS artifacts_stable_id ON TABLE artifacts FIELDS stable_id UNIQUE;
    DEFINE INDEX IF NOT EXISTS artifacts_content_hash ON TABLE artifacts FIELDS content_hash;

    DEFINE TABLE IF NOT EXISTS events SCHEMAFULL;
    DEFINE FIELD IF NOT EXISTS stable_id ON TABLE events TYPE string;
    DEFINE FIELD IF NOT EXISTS origin_node_id ON TABLE events TYPE string;
    DEFINE FIELD IF NOT EXISTS subject ON TABLE events TYPE option<record>;
    DEFINE FIELD IF NOT EXISTS event_kind ON TABLE events TYPE string;
    DEFINE FIELD IF NOT EXISTS payload ON TABLE events TYPE object FLEXIBLE;
    DEFINE FIELD IF NOT EXISTS occurred_at ON TABLE events TYPE datetime;
    DEFINE FIELD IF NOT EXISTS immutable ON TABLE events TYPE bool;
    DEFINE FIELD IF NOT EXISTS created_at ON TABLE events TYPE datetime;
    DEFINE INDEX IF NOT EXISTS events_stable_id ON TABLE events FIELDS stable_id UNIQUE;
    DEFINE INDEX IF NOT EXISTS events_subject_kind ON TABLE events FIELDS subject, event_kind;

    DEFINE TABLE IF NOT EXISTS sync_ledger SCHEMAFULL;
    DEFINE FIELD IF NOT EXISTS stable_id ON TABLE sync_ledger TYPE string;
    DEFINE FIELD IF NOT EXISTS origin_node_id ON TABLE sync_ledger TYPE string;
    DEFINE FIELD IF NOT EXISTS peer_node_id ON TABLE sync_ledger TYPE string;
    DEFINE FIELD IF NOT EXISTS direction ON TABLE sync_ledger TYPE string;
    DEFINE FIELD IF NOT EXISTS table_name ON TABLE sync_ledger TYPE string;
    DEFINE FIELD IF NOT EXISTS record_stable_id ON TABLE sync_ledger TYPE string;
    DEFINE FIELD IF NOT EXISTS record_revision ON TABLE sync_ledger TYPE option<string>;
    DEFINE FIELD IF NOT EXISTS artifact_hash ON TABLE sync_ledger TYPE option<string>;
    DEFINE FIELD IF NOT EXISTS status ON TABLE sync_ledger TYPE string;
    DEFINE FIELD IF NOT EXISTS detail ON TABLE sync_ledger TYPE object FLEXIBLE;
    DEFINE FIELD IF NOT EXISTS synced_at ON TABLE sync_ledger TYPE datetime;
    DEFINE INDEX IF NOT EXISTS sync_ledger_stable_id ON TABLE sync_ledger FIELDS stable_id UNIQUE;
    DEFINE INDEX IF NOT EXISTS sync_ledger_record ON TABLE sync_ledger FIELDS table_name, record_stable_id;

    DEFINE TABLE IF NOT EXISTS ui_state SCHEMAFULL;
    DEFINE FIELD IF NOT EXISTS scope ON TABLE ui_state TYPE string;
    DEFINE FIELD IF NOT EXISTS key ON TABLE ui_state TYPE string;
    DEFINE FIELD IF NOT EXISTS value ON TABLE ui_state TYPE object FLEXIBLE;
    DEFINE FIELD IF NOT EXISTS updated_at ON TABLE ui_state TYPE datetime;
    DEFINE INDEX IF NOT EXISTS ui_state_scope_key ON TABLE ui_state FIELDS scope, key UNIQUE;
    "#
}

#[derive(Debug, Error)]
pub enum DatabaseError {
    #[error("failed to create data directory at {path}: {source}")]
    CreateDir {
        path: PathBuf,
        source: std::io::Error,
    },
    #[error("surrealdb error: {0}")]
    Surreal(#[from] surrealdb::Error),
    #[error("surrealdb client was not available after initialization")]
    ClientUnavailable,
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn schema_bootstrap_query_executes_on_embedded_surrealdb() {
        tauri::async_runtime::block_on(async {
            let temp_dir = tempdir().expect("create tempdir for surrealdb schema test");
            let endpoint = temp_dir.path().join("surrealdb");

            let db = Surreal::new::<RocksDb>(endpoint.to_string_lossy().into_owned())
                .await
                .expect("open embedded surrealdb");
            db.use_ns("test")
                .use_db("schema")
                .await
                .expect("select ns/db");
            db.query(schema_bootstrap_query())
                .await
                .expect("apply schema bootstrap query");
        });
    }
}
