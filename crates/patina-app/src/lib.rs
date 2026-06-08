mod adapters;
mod application;
mod commands;
mod database;
mod models;
mod state;

use std::fs;

use tauri::Manager;

use crate::state::{default_reference_workspace_root, AppState};

pub fn run() {
    tauri::Builder::default()
        .setup(|app| {
            let app_data_dir = app
                .path()
                .app_data_dir()
                .expect("failed to resolve the application data directory");
            fs::create_dir_all(&app_data_dir)
                .expect("failed to create the application data directory");

            let reference_workspace_root = default_reference_workspace_root();
            let campaign_checkpoint = reference_workspace_root
                .join("docs")
                .join("active")
                .join("app")
                .join("PATINA_DESKTOP_APP_CAMPAIGN_2026-04-23.md");

            app.manage(AppState::new(
                app_data_dir,
                reference_workspace_root,
                campaign_checkpoint,
            ));
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::load_app_overview,
            commands::load_app_snapshot_json,
            commands::load_data_foundation,
            commands::load_data_flow_lab,
            commands::load_autoemulate_lab_json,
            commands::connect_data_store,
            commands::load_workspace_catalog,
            commands::load_workspace_catalog_json,
            commands::load_workspace_run_json,
            commands::load_structure_preview_json,
            commands::sync_workspace_catalog_to_store,
            commands::sync_workspace_catalog_to_store_json,
            commands::preview_surface_lab,
            commands::preview_surface_lab_json,
            commands::preview_material_flow_json,
            commands::load_topology_lab,
            commands::load_topology_lab_json
        ])
        .run(tauri::generate_context!())
        .expect("failed to run patina-app");
}
