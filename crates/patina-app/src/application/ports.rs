use async_trait::async_trait;
use std::path::Path;

use anyhow::Result;

use crate::models::{CatalogSyncReport, WorkspaceCatalog};

pub trait WorkspaceCatalogPort: Send + Sync {
    fn load_active_runs(&self, runs_root: &Path) -> Result<WorkspaceCatalog>;
}

#[async_trait]
pub trait WorkspaceStorePort: Send + Sync {
    async fn sync_workspace_catalog(
        &self,
        workspace_root: &Path,
        catalog: &WorkspaceCatalog,
    ) -> Result<CatalogSyncReport>;
}
