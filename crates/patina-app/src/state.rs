use std::path::{Path, PathBuf};

use crate::adapters::LocalWorkspaceCatalogAdapter;
use crate::application::WorkspaceCatalogService;
use crate::database::DatabaseState;
use crate::models::{
    AppOverview, AuthorityZone, DataFoundation, PhaseStatus, SaintTranslation, StackDecision,
    SyncPrinciple, TableBlueprint,
};

pub struct AppState {
    database: DatabaseState,
    workspace_catalog: WorkspaceCatalogService,
    reference_workspace_root: PathBuf,
    campaign_checkpoint: PathBuf,
}

impl AppState {
    pub fn new(
        app_data_dir: PathBuf,
        reference_workspace_root: PathBuf,
        campaign_checkpoint: PathBuf,
    ) -> Self {
        Self {
            database: DatabaseState::new(app_data_dir),
            workspace_catalog: WorkspaceCatalogService::new(Box::new(LocalWorkspaceCatalogAdapter)),
            reference_workspace_root,
            campaign_checkpoint,
        }
    }

    pub fn database(&self) -> &DatabaseState {
        &self.database
    }

    pub fn reference_workspace_root(&self) -> &Path {
        &self.reference_workspace_root
    }

    pub fn workspace_catalog(&self) -> &WorkspaceCatalogService {
        &self.workspace_catalog
    }

    pub fn overview(&self) -> AppOverview {
        AppOverview {
            app_name: "PATINA Desktop App".to_string(),
            campaign_checkpoint: self.campaign_checkpoint.to_string_lossy().into_owned(),
            reference_workspace_root: self
                .reference_workspace_root
                .to_string_lossy()
                .into_owned(),
            local_first_claims: vec![
                "No Flask API tier is required for the desktop shell.".to_string(),
                "No central task queue is required for normal single-machine use.".to_string(),
                "SurrealDB is the portable authority for syncable scientific records, workflow intent, and provenance."
                    .to_string(),
                "Heavyweight artifacts stay on disk, but only through database-anchored manifests, hashes, and semantic roles."
                    .to_string(),
                "Rust crates remain the source of workflow semantics and core DTO definitions."
                    .to_string(),
            ],
            stack: vec![
                StackDecision {
                    area: "Desktop shell".to_string(),
                    choice: "Tauri v2".to_string(),
                    rationale:
                        "Rust-native backend with a small desktop wrapper around a web UI."
                            .to_string(),
                },
                StackDecision {
                    area: "Frontend".to_string(),
                    choice: "Svelte + Vite".to_string(),
                    rationale:
                        "Lowest-friction path to a MatterViz-shaped interface because MatterViz is Svelte-native."
                            .to_string(),
                },
                StackDecision {
                    area: "Scientific UI".to_string(),
                    choice: "MatterViz".to_string(),
                    rationale:
                        "Target toolkit for structure, trajectory, composition, and scientific plot surfaces."
                            .to_string(),
                },
                StackDecision {
                    area: "Persistence".to_string(),
                    choice: "Embedded SurrealDB on RocksDB".to_string(),
                    rationale:
                        "Authoritative local scientific application graph and provenance ledger without a separate server."
                            .to_string(),
                },
            ],
            saint_translation: vec![
                SaintTranslation {
                    saint_shape: "Flask UI + API + Celery split".to_string(),
                    patina_direction: "One Tauri app with Rust commands and events.".to_string(),
                },
                SaintTranslation {
                    saint_shape: "MongoEngine document store".to_string(),
                    patina_direction: "Typed Rust DTOs with embedded SurrealDB state.".to_string(),
                },
                SaintTranslation {
                    saint_shape: "Centralized node assumptions".to_string(),
                    patina_direction: "Local-first workspace and per-machine execution.".to_string(),
                },
                SaintTranslation {
                    saint_shape: "Jmol-era structure views".to_string(),
                    patina_direction: "MatterViz-oriented structure and data theater.".to_string(),
                },
            ],
            phases: vec![
                PhaseStatus {
                    name: "Phase 0 - objectives and boundaries".to_string(),
                    status: "complete".to_string(),
                    objective: "Fix the architecture direction before more UI code lands."
                        .to_string(),
                },
                PhaseStatus {
                    name: "Phase 1 - desktop shell scaffold".to_string(),
                    status: "in_progress".to_string(),
                    objective: "Stand up the Tauri crate, command surface, and initial frontend."
                        .to_string(),
                },
                PhaseStatus {
                    name: "Phase 2 - local workspace catalog".to_string(),
                    status: "planned".to_string(),
                    objective:
                        "Stand up the authoritative local graph for structures, run specs, runs, evaluations, and artifacts."
                            .to_string(),
                },
                PhaseStatus {
                    name: "Phase 3 - launch and monitoring".to_string(),
                    status: "planned".to_string(),
                    objective: "Drive Rust workflows through typed desktop commands and events."
                        .to_string(),
                },
                PhaseStatus {
                    name: "Phase 4 - visualization theater".to_string(),
                    status: "planned".to_string(),
                    objective: "Mount MatterViz surfaces for structures, trajectories, and plots."
                        .to_string(),
                },
            ],
        }
    }

    pub fn data_foundation(&self) -> DataFoundation {
        DataFoundation {
            thesis:
                "SurrealDB should own the portable scientific application graph, while large run artifacts remain external but are anchored by database manifests, hashes, and semantic roles."
                    .to_string(),
            authority_zones: vec![
                AuthorityZone {
                    zone: "Portable scientific records".to_string(),
                    authority: "SurrealDB".to_string(),
                    rationale:
                        "Structures, evaluations, workflow intent, runs, lineage, and sync receipts must stay queryable and exportable across nodes."
                            .to_string(),
                },
                AuthorityZone {
                    zone: "Heavyweight generated payloads".to_string(),
                    authority: "Filesystem with DB anchors".to_string(),
                    rationale:
                        "Checkpoints, logs, trajectories, dreadnaut traces, and rendered outputs are better stored as files but must be fingerprinted and owned by database records."
                            .to_string(),
                },
                AuthorityZone {
                    zone: "Presentation caches".to_string(),
                    authority: "Disposable local cache".to_string(),
                    rationale:
                        "Thumbnails, search helpers, and view-oriented summaries should be rebuildable and never outrank the scientific record."
                            .to_string(),
                },
            ],
            sync_principles: vec![
                SyncPrinciple {
                    name: "Stable application identity".to_string(),
                    detail:
                        "Every syncable record family needs a durable stable_id plus an origin_node_id."
                            .to_string(),
                },
                SyncPrinciple {
                    name: "Revision-aware mutation".to_string(),
                    detail:
                        "Mutable records need explicit revision tokens and immutable provenance events instead of silent overwrites."
                            .to_string(),
                },
                SyncPrinciple {
                    name: "Artifact-addressed sharing".to_string(),
                    detail:
                        "Artifacts must carry semantic roles, content hashes, and owner references so later master-node sharing is selective and auditable."
                            .to_string(),
                },
                SyncPrinciple {
                    name: "Execution is not the same as intent".to_string(),
                    detail:
                        "Logical runs and run specifications should remain distinct from run attempts on local or remote execution targets."
                            .to_string(),
                },
            ],
            table_blueprint: vec![
                TableBlueprint {
                    table: "nodes".to_string(),
                    role: "Node identity and machine profile".to_string(),
                    authority: "syncable".to_string(),
                    notes:
                        "Foundation for local-first operation today and master-node federation later."
                            .to_string(),
                },
                TableBlueprint {
                    table: "workspaces".to_string(),
                    role: "Logical project roots and ownership".to_string(),
                    authority: "syncable".to_string(),
                    notes:
                        "Decouples the durable project record from whichever machine-local path currently hosts artifacts."
                            .to_string(),
                },
                TableBlueprint {
                    table: "structures".to_string(),
                    role: "Canonical structure payloads and lineage".to_string(),
                    authority: "syncable".to_string(),
                    notes:
                        "Natural home for `patina-types::StructureRecord` plus portable scientific metadata."
                            .to_string(),
                },
                TableBlueprint {
                    table: "surfaces".to_string(),
                    role: "Surface definitions tied to parent structures".to_string(),
                    authority: "syncable".to_string(),
                    notes:
                        "Needed for surface-science flows without inheriting SAINT's centralized assumptions."
                            .to_string(),
                },
                TableBlueprint {
                    table: "run_specs".to_string(),
                    role: "Workflow intent, requested backend policy, and input refs".to_string(),
                    authority: "syncable".to_string(),
                    notes:
                        "Captures what the scientist asked for before any local or remote attempt starts."
                            .to_string(),
                },
                TableBlueprint {
                    table: "runs".to_string(),
                    role: "Logical run lifecycle".to_string(),
                    authority: "syncable".to_string(),
                    notes:
                        "Run identity should survive retries, imports, and movement between execution targets."
                            .to_string(),
                },
                TableBlueprint {
                    table: "run_attempts".to_string(),
                    role: "Concrete execution attempts".to_string(),
                    authority: "syncable".to_string(),
                    notes:
                        "Separates scientific intent from the reality of retries, crashes, and remote execution later."
                            .to_string(),
                },
                TableBlueprint {
                    table: "execution_targets".to_string(),
                    role: "Local machine or future SSH/HPC target".to_string(),
                    authority: "syncable".to_string(),
                    notes:
                        "Makes later cluster execution an additive capability rather than an architectural rewrite."
                            .to_string(),
                },
                TableBlueprint {
                    table: "evaluations".to_string(),
                    role: "Accepted evaluation summaries and relaxed-state provenance".to_string(),
                    authority: "syncable".to_string(),
                    notes:
                        "Natural home for `patina-types::EvaluationRecord` and accepted scientific summaries."
                            .to_string(),
                },
                TableBlueprint {
                    table: "artifacts".to_string(),
                    role: "Artifact manifests, hashes, and ownership".to_string(),
                    authority: "anchoring".to_string(),
                    notes:
                        "This is how files become shareable scientific evidence without being inlined into the database."
                            .to_string(),
                },
                TableBlueprint {
                    table: "events".to_string(),
                    role: "Immutable provenance stream".to_string(),
                    authority: "syncable".to_string(),
                    notes:
                        "Critical for explaining how records changed across semi-offline nodes."
                            .to_string(),
                },
                TableBlueprint {
                    table: "sync_ledger".to_string(),
                    role: "Import/export and share receipts".to_string(),
                    authority: "syncable".to_string(),
                    notes:
                        "Tracks what was shared with peers or a future master node and with which revision."
                            .to_string(),
                },
                TableBlueprint {
                    table: "ui_state".to_string(),
                    role: "Local presentation preferences".to_string(),
                    authority: "local_only".to_string(),
                    notes:
                        "Useful, but intentionally outside the scientific truth boundary."
                            .to_string(),
                },
            ],
        }
    }
}

pub fn default_reference_workspace_root() -> PathBuf {
    let manifest_dir = Path::new(env!("CARGO_MANIFEST_DIR"));
    manifest_dir
        .ancestors()
        .nth(2)
        .map(Path::to_path_buf)
        .unwrap_or_else(|| manifest_dir.to_path_buf())
}
