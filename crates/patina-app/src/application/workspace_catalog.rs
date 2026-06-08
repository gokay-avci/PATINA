use std::path::Path;

use anyhow::Result;

use crate::application::ports::WorkspaceCatalogPort;
use crate::models::WorkspaceCatalog;

pub struct WorkspaceCatalogService {
    catalog_port: Box<dyn WorkspaceCatalogPort>,
}

impl WorkspaceCatalogService {
    pub fn new(catalog_port: Box<dyn WorkspaceCatalogPort>) -> Self {
        Self { catalog_port }
    }

    pub fn load_reference_workspace_catalog(
        &self,
        workspace_root: &Path,
    ) -> Result<WorkspaceCatalog> {
        let runs_root = workspace_root.join("runs").join("active");
        self.catalog_port.load_active_runs(&runs_root)
    }
}
