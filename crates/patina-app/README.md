# patina-app

Local-first Tauri desktop application scaffold for PATINA.

Current scope:

- opens a Tauri v2 shell around the Rust workspace
- prepares an embedded SurrealDB datastore contract
- exposes a first Rust command surface for app overview and local run discovery
- provides a Svelte/Vite desktop UI shell shaped for later MatterViz integration

Initial development flow:

```bash
cd crates/patina-app
npm install
npm run tauri dev
```

Current notes:

- the frontend dependency stack is declared locally in this crate
- the MatterViz package is included in the frontend stack but not mounted yet
- the app currently reads the repository's local `runs/active` tree as a development reference
