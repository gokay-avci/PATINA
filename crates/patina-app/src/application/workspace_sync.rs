use std::path::Path;

use anyhow::Result;

use crate::application::ports::{WorkspaceCatalogPort, WorkspaceStorePort};
use crate::models::CatalogSyncReport;

pub struct WorkspaceCatalogSyncService;

impl WorkspaceCatalogSyncService {
    pub async fn sync_reference_workspace_catalog<C, S>(
        &self,
        workspace_root: &Path,
        catalog_port: &C,
        store_port: &S,
    ) -> Result<CatalogSyncReport>
    where
        C: WorkspaceCatalogPort + ?Sized,
        S: WorkspaceStorePort + ?Sized,
    {
        let runs_root = workspace_root.join("runs").join("active");
        let catalog = catalog_port.load_active_runs(&runs_root)?;
        store_port
            .sync_workspace_catalog(workspace_root, &catalog)
            .await
    }
}
