<script>
  import { onMount } from 'svelte';
  import { invoke, isTauri } from '@tauri-apps/api/core';
  import { Plot, Dot, Line, GridY, AxisX, AxisY, Pointer, Text, Cell } from 'svelteplot';
  import LazyMatterVizStructure from './LazyMatterVizStructure.svelte';

  const workspaceRoot = '<patina-workspace>';

  const fallbackOverview = {
    app_name: 'PATINA Desktop App',
    campaign_checkpoint: `${workspaceRoot}/docs/active/app/PATINA_DESKTOP_APP_CAMPAIGN_2026-04-23.md`
  };

  const fallbackDatabase = {
    engine: 'surrealdb+rocksdb',
    namespace: 'patina',
    database: 'desktop',
    path: '~/Library/Application Support/com.patina.app/surrealdb',
    connected: false
  };

  const fallbackDataFoundation = {
    thesis:
      'SurrealDB should expose the portable scientific record for runs, structures, surfaces, provenance, and sync receipts while heavyweight files remain external artifacts.',
    table_blueprint: [
      { table: 'nodes', role: 'node identity and machine profile' },
      { table: 'workspaces', role: 'logical project roots and ownership' },
      { table: 'structures', role: 'portable structure records and lineage' },
      { table: 'surfaces', role: 'surface definitions and slab-derived records' },
      { table: 'run_specs', role: 'workflow intent before execution' },
      { table: 'runs', role: 'logical run lifecycle and status' },
      { table: 'evaluations', role: 'accepted energies and summaries' },
      { table: 'artifacts', role: 'file manifests, hashes, and ownership' },
      { table: 'events', role: 'immutable provenance stream' },
      { table: 'sync_ledger', role: 'import/export and share receipts' }
    ]
  };

  const fallbackCatalog = {
    runs_root: `${workspaceRoot}/runs/active`,
    discovered: 2,
    runs: [
      {
        run_name: 'ti3n4_janus_mace_3g10p_20260421_115140',
        path: `${workspaceRoot}/runs/active/ti3n4_janus_mace_3g10p_20260421_115140`,
        catalog_status: 'ready',
        catalog_notes: [],
        has_manifest: true,
        has_checkpoint: true,
        workflow_owner: 'persistent_daemon_ga',
        workflow_scope: 'Nanocluster genetic search with persistent daemon workers.',
        system: '(Ti3N4)3_cluster',
        backend: 'janus_mace',
        runtime_engine: 'mace',
        lane_mode: 'standalone_capable',
        run_spec: {
          population_size: 10,
          requested_generations: 3,
          seed: 16,
          temperature: 625.0,
          step_size: 0.8,
          parallel_contract: 'persistent_daemon_workers_only'
        },
        best_structure: {
          label: 'ga_gen_0002_0007',
          formula: 'Ti9N12',
          site_count: 21,
          dimensionality: '0D',
          energy: -182.97277176606303
        },
        structure_preview: {
          sites: [
            { species: [{ element: 'Ti', occu: 1, oxidation_state: 0 }], abc: [0.22, 0.34, 0.44], xyz: [1.8, 2.2, 3.1], label: 'Ti1', properties: {} },
            { species: [{ element: 'N', occu: 1, oxidation_state: 0 }], abc: [0.38, 0.53, 0.48], xyz: [3.2, 3.7, 3.4], label: 'N2', properties: {} },
            { species: [{ element: 'Ti', occu: 1, oxidation_state: 0 }], abc: [0.54, 0.45, 0.60], xyz: [4.5, 3.1, 4.4], label: 'Ti3', properties: {} },
            { species: [{ element: 'N', occu: 1, oxidation_state: 0 }], abc: [0.70, 0.62, 0.36], xyz: [5.7, 4.4, 2.8], label: 'N4', properties: {} }
          ],
          lattice: { matrix: [[8, 0, 0], [0, 8, 0], [0, 0, 8]], a: 8, b: 8, c: 8, alpha: 90, beta: 90, gamma: 90, volume: 512, pbc: [false, false, false] }
        },
        artifacts: {
          manifest_path: `${workspaceRoot}/runs/active/ti3n4_janus_mace_3g10p_20260421_115140/manifest.json`,
          declared_artifacts: 14,
          latest_checkpoint: `${workspaceRoot}/runs/active/ti3n4_janus_mace_3g10p_20260421_115140/raw/rust_ga_checkpoint_latest.json`,
          generation_state_latest: `${workspaceRoot}/runs/active/ti3n4_janus_mace_3g10p_20260421_115140/raw/ga_generation_state_latest.json`,
          structure_exports: null,
          controller_trace: `${workspaceRoot}/runs/active/ti3n4_janus_mace_3g10p_20260421_115140/traces/controller_trace.csv`,
          search_summary: `${workspaceRoot}/runs/active/ti3n4_janus_mace_3g10p_20260421_115140/raw/search_summary.json`
        }
      },
      {
        run_name: 'ti3n4_monolithic_gulp_3g10p_20260421_114907',
        path: `${workspaceRoot}/runs/active/ti3n4_monolithic_gulp_3g10p_20260421_114907`,
        catalog_status: 'ready',
        catalog_notes: [],
        has_manifest: true,
        has_checkpoint: true,
        workflow_owner: 'scott_monolithic_ga',
        workflow_scope: 'Nanocluster GA with monolithic Scott evaluator/runtime generation execution.',
        system: '(Ti3N4)3_cluster',
        backend: 'scott_runtime',
        runtime_engine: 'gulp',
        lane_mode: 'standalone_capable',
        run_spec: {
          population_size: 10,
          requested_generations: 3,
          seed: 16,
          temperature: 625.0,
          step_size: 0.8,
          parallel_contract: 'monolithic_scott_runtime_generation_dispatch'
        },
        best_structure: {
          label: 'ga_repop_fallback_0009',
          formula: 'Ti9N12',
          site_count: 21,
          dimensionality: '0D',
          energy: -24467.65855811
        },
        structure_preview: {
          sites: [
            { species: [{ element: 'Ti', occu: 1, oxidation_state: 0 }], abc: [0.18, 0.28, 0.40], xyz: [1.4, 2.1, 3.0], label: 'Ti1', properties: {} },
            { species: [{ element: 'N', occu: 1, oxidation_state: 0 }], abc: [0.34, 0.44, 0.50], xyz: [2.8, 3.4, 3.9], label: 'N2', properties: {} },
            { species: [{ element: 'N', occu: 1, oxidation_state: 0 }], abc: [0.48, 0.58, 0.42], xyz: [4.0, 4.6, 3.3], label: 'N3', properties: {} },
            { species: [{ element: 'Ti', occu: 1, oxidation_state: 0 }], abc: [0.66, 0.48, 0.62], xyz: [5.2, 3.8, 4.8], label: 'Ti4', properties: {} }
          ],
          lattice: { matrix: [[8, 0, 0], [0, 8, 0], [0, 0, 8]], a: 8, b: 8, c: 8, alpha: 90, beta: 90, gamma: 90, volume: 512, pbc: [false, false, false] }
        },
        artifacts: {
          manifest_path: `${workspaceRoot}/runs/active/ti3n4_monolithic_gulp_3g10p_20260421_114907/manifest.json`,
          declared_artifacts: 16,
          latest_checkpoint: `${workspaceRoot}/runs/active/ti3n4_monolithic_gulp_3g10p_20260421_114907/raw/rust_ga_checkpoint_latest.json`,
          generation_state_latest: `${workspaceRoot}/runs/active/ti3n4_monolithic_gulp_3g10p_20260421_114907/raw/ga_generation_state_latest.json`,
          structure_exports: null,
          controller_trace: `${workspaceRoot}/runs/active/ti3n4_monolithic_gulp_3g10p_20260421_114907/traces/controller_trace.csv`,
          search_summary: `${workspaceRoot}/runs/active/ti3n4_monolithic_gulp_3g10p_20260421_114907/raw/search_summary.json`
        }
      }
    ]
  };

  const fallbackDataFlow = {
    table_counts: [
      { table: 'nodes', count: 1, role: 'Machine identities and ownership roots' },
      { table: 'workspaces', count: 1, role: 'Logical project roots decoupled from transient paths' },
      { table: 'structures', count: fallbackCatalog.runs.filter((run) => run.best_structure).length, role: 'Portable structure records that downstream tools can reuse' },
      { table: 'surfaces', count: 0, role: 'Surface definitions and slab-derived entities' },
      { table: 'run_specs', count: fallbackCatalog.runs.length, role: 'Workflow intent before execution' },
      { table: 'runs', count: fallbackCatalog.runs.length, role: 'Logical run lifecycle and status' },
      { table: 'run_attempts', count: 0, role: 'Concrete execution attempts on local or remote targets' },
      { table: 'evaluations', count: 0, role: 'Scientific summaries, energies, and convergence state' },
      {
        table: 'artifacts',
        count: fallbackCatalog.runs.reduce((sum, run) => sum + run.artifacts.declared_artifacts, 0),
        role: 'Filesystem payloads anchored by semantic role and hash'
      },
      { table: 'events', count: 0, role: 'Immutable provenance events' },
      { table: 'sync_ledger', count: 0, role: 'Import/export receipts and future node exchange' }
    ],
    import_pipeline: [
      {
        stage: '1. Discover workspace files',
        detail: 'The local adapter reads run manifests, generation-state JSON, checkpoints, and declared artifact paths from runs/active.'
      },
      {
        stage: '2. Normalize into typed records',
        detail: 'PATINA structures, evaluations, run specs, and artifact digests are normalized before persistence so the DB stores portable scientific meaning instead of raw directory assumptions.'
      },
      {
        stage: '3. Upsert SurrealDB authority tables',
        detail: 'sync_workspace_catalog_to_store writes workspaces, runs, structures, evaluations, artifacts, events, and sync receipts with stable ids and revision tokens.'
      },
      {
        stage: '4. Reuse records across lanes',
        detail: 'Cluster browsing, topology inspection, and future sampling/surface launchers can now consume the same canonical structure records instead of reparsing raw files independently.'
      }
    ],
    export_pipeline: [
      {
        stage: '1. Query a durable structure identity',
        detail: 'The DB answers which structure, run, evaluation, and artifact set you mean via stable ids, provenance, and hashes.'
      },
      {
        stage: '2. Materialize external files only when needed',
        detail: 'Heavy payloads stay on disk; the DB points to the owning files through artifact manifests and content hashes.'
      },
      {
        stage: '3. Hand structures to the next workflow',
        detail: 'Surface generation, topology analysis, follow-on sampling, or peer-node sharing should consume DB-selected structure records and resolve filesystem artifacts only at execution time.'
      },
      {
        stage: '4. Emit export and sync receipts',
        detail: 'sync_ledger is the place to record that a structure or artifact left this node, was imported elsewhere, or was promoted into another workflow.'
      }
    ],
    workflow_chains: [
      {
        name: 'Cluster search -> topology identity',
        steps: [
          'runs/active/* -> catalog sync',
          'structures + evaluations in SurrealDB',
          'selected structure -> dreadnaut/hashkey analysis',
          'events + sync receipts anchor the result'
        ]
      },
      {
        name: 'Cluster search -> follow-on sampling',
        steps: [
          'best structures imported into structures table',
          'run_specs record the follow-on intent',
          'new runs / run_attempts execute basin hopping, annealing, or lid exploration',
          'evaluations and artifacts remain linked to the parent identity'
        ]
      }
    ],
    current_capabilities: [
      'The app can already import tracked workspace runs into authoritative SurrealDB tables through the sync command.',
      'Structures are not just thumbnails: the DB stores portable structure payloads, formulas, dimensionality, lineage hooks, and links to evaluations.',
      'Artifacts remain external files, but the DB records their semantic role, path, size, hash, and owner so downstream workflows can find them reliably.'
    ],
    current_gaps: [
      'There is not yet a dedicated app command that ingests an arbitrary user-picked CIF/XYZ directly into the structures table outside the workspace sync path.',
      'There is not yet a dedicated app export command that serializes a chosen structures-row into a portable exchange bundle with CIF/XYZ plus sync receipt.',
      'run_attempts and execution_targets are in the schema but not yet fully surfaced by the current UI because the app has not wired real launch/monitor flows end-to-end.'
    ]
  };

  const fallbackAutoemulateLab = {
    analysis_root: `${workspaceRoot}/runs/analysis`,
    discovered: 4,
    experiments: [
      {
        experiment_name: 'ti3n4_janus_pair_emulate',
        artifact_dir: `${workspaceRoot}/runs/analysis/ti3n4_janus_pair_emulate/campaign-ti3n4-janus-pair-branch-ga-1-1777365159729589000`,
        campaign_id: 'campaign-ti3n4-janus-pair',
        branch_id: 'branch-ga-1',
        ga_run_name: 'ti3n4_janus_mace_3g10p_20260421_115140',
        feature_family: 'pair_distance_signature',
        feature_version: 'v1',
        feature_count: 36,
        training_observation_count: 34,
        pending_candidate_count: 10,
        selected_model_name: 'Whitened Full-Covariance SVGP',
        model_variant: 'whitened_svgp',
        fidelity: 'janus_mace_low',
        incumbent_target: -182.97277176606303,
        best_ranked_candidate_id: 'ga_pending_0000',
        highest_uncertainty_candidate_id: 'ga_pending_0005',
        top_acquisition_score: -0.6328108267339807,
        top_uncertainty_score: 2.762872220332274,
        checkpoint_path: `${workspaceRoot}/runs/analysis/ti3n4_janus_pair_emulate/checkpoints/campaign-ti3n4-janus-pair/branch-ga-1/whitened_svgp-pair_distance_signature-v1-36f/svgp_model.pt`,
        training_target_summary: { min: -182.97277176606303, max: -167.9195824403846, mean: -177.29642481196524 },
        predicted_target_summary: { min: -181.24664306640625, max: -175.43650817871094, mean: -178.7270980834961 },
        predicted_variance_summary: { min: 4.295845031738281, max: 7.633462905883789, mean: 5.465451955795288 },
        acquisition_score_summary: { min: -6.391491034364236, max: -0.6328108267339807, mean: -3.0801016909175543 },
        uncertainty_score_summary: { min: 2.0726420413902353, max: 2.762872220332274, mean: 2.3311439832987704 },
        top_ranked_candidates: [
          { candidate_id: 'ga_pending_0000', label: '01_ga_gen_0002_0007', rank: 1, predicted_mean: -181.24664306640625, predicted_variance: 4.781375885009766, acquisition_score: -0.6328108267339807, uncertainty_score: 2.186635745845605 },
          { candidate_id: 'ga_pending_0001', label: '02_ga_gen_0003_0003', rank: 2, predicted_mean: -180.4613037109375, predicted_variance: 5.285083293914795, acquisition_score: -1.3620026039358517, uncertainty_score: 2.2989309023793636 },
          { candidate_id: 'ga_pending_0002', label: '03_ga_gen_0003_0002', rank: 3, predicted_mean: -179.64480590820312, predicted_variance: 5.194660663604736, acquisition_score: -2.1883759465755093, uncertainty_score: 2.2791798225687976 }
        ],
        highest_uncertainty_candidates: [
          { candidate_id: 'ga_pending_0005', label: '06_ga_repop_0008_04', rank: 9, predicted_mean: -177.48980712890625, predicted_variance: 7.633462905883789, acquisition_score: -4.101528526990647, uncertainty_score: 2.762872220332274 }
        ],
        predictions: [
          { candidate_id: 'ga_pending_0000', label: '01_ga_gen_0002_0007', rank: 1, predicted_mean: -181.24664306640625, predicted_variance: 4.781375885009766, acquisition_score: -0.6328108267339807, uncertainty_score: 2.186635745845605, features: [21, 4.04, 0.51] },
          { candidate_id: 'ga_pending_0001', label: '02_ga_gen_0003_0003', rank: 2, predicted_mean: -180.4613037109375, predicted_variance: 5.285083293914795, acquisition_score: -1.3620026039358517, uncertainty_score: 2.2989309023793636, features: [21, 4.15, 0.49] },
          { candidate_id: 'ga_pending_0002', label: '03_ga_gen_0003_0002', rank: 3, predicted_mean: -179.64480590820312, predicted_variance: 5.194660663604736, acquisition_score: -2.1883759465755093, uncertainty_score: 2.2791798225687976, features: [21, 4.22, 0.48] },
          { candidate_id: 'ga_pending_0005', label: '06_ga_repop_0008_04', rank: 9, predicted_mean: -177.48980712890625, predicted_variance: 7.633462905883789, acquisition_score: -4.101528526990647, uncertainty_score: 2.762872220332274, features: [21, 4.72, 0.42] }
        ],
        feature_names: ['atom_count', 'pair_distance_mean', 'pair_fraction:N-Ti']
      },
      {
        experiment_name: 'ti3n4_janus_simple_emulate',
        artifact_dir: `${workspaceRoot}/runs/analysis/ti3n4_janus_simple_emulate/campaign-ti3n4-janus-simple-branch-ga-1-1777365189579906000`,
        campaign_id: 'campaign-ti3n4-janus-simple',
        branch_id: 'branch-ga-1',
        ga_run_name: 'ti3n4_janus_mace_3g10p_20260421_115140',
        feature_family: 'simple_structure_statistics',
        feature_version: 'v1',
        feature_count: 17,
        training_observation_count: 34,
        pending_candidate_count: 10,
        selected_model_name: 'Whitened Full-Covariance SVGP',
        model_variant: 'whitened_svgp',
        fidelity: 'janus_mace_low',
        incumbent_target: -182.97277176606303,
        best_ranked_candidate_id: 'ga_pending_0000',
        highest_uncertainty_candidate_id: 'ga_pending_0005',
        top_acquisition_score: -2.284457447531371,
        top_uncertainty_score: 3.230650005306534,
        checkpoint_path: null,
        training_target_summary: null,
        predicted_target_summary: null,
        predicted_variance_summary: null,
        acquisition_score_summary: null,
        uncertainty_score_summary: { min: 2.0, max: 3.230650005306534, mean: 2.6 },
        top_ranked_candidates: [],
        highest_uncertainty_candidates: [],
        predictions: [],
        feature_names: []
      },
      {
        experiment_name: 'ti3n4_janus_dscribe_emulate',
        artifact_dir: `${workspaceRoot}/runs/analysis/ti3n4_janus_dscribe_emulate/campaign-ti3n4-janus-dscribe-branch-ga-1-1777365193415475000`,
        campaign_id: 'campaign-ti3n4-janus-dscribe',
        branch_id: 'branch-ga-1',
        ga_run_name: 'ti3n4_janus_mace_3g10p_20260421_115140',
        feature_family: 'dscribe_soap_global',
        feature_version: 'v1',
        feature_count: 390,
        training_observation_count: 34,
        pending_candidate_count: 10,
        selected_model_name: 'Whitened Full-Covariance SVGP',
        model_variant: 'whitened_svgp',
        fidelity: 'janus_mace_low',
        incumbent_target: -182.97277176606303,
        best_ranked_candidate_id: 'ga_pending_0003',
        highest_uncertainty_candidate_id: 'ga_pending_0008',
        top_acquisition_score: -2.5488880021931424,
        top_uncertainty_score: 3.1820413560751315,
        checkpoint_path: null,
        training_target_summary: null,
        predicted_target_summary: null,
        predicted_variance_summary: null,
        acquisition_score_summary: null,
        uncertainty_score_summary: { min: 2.0, max: 3.1820413560751315, mean: 2.7 },
        top_ranked_candidates: [],
        highest_uncertainty_candidates: [],
        predictions: [],
        feature_names: []
      },
      {
        experiment_name: 'ti3n4_janus_featomic_emulate',
        artifact_dir: `${workspaceRoot}/runs/analysis/ti3n4_janus_featomic_emulate/campaign-ti3n4-janus-featomic-branch-ga-1-1777365192141257000`,
        campaign_id: 'campaign-ti3n4-janus-featomic',
        branch_id: 'branch-ga-1',
        ga_run_name: 'ti3n4_janus_mace_3g10p_20260421_115140',
        feature_family: 'featomic_soap_global',
        feature_version: 'v1',
        feature_count: 735,
        training_observation_count: 34,
        pending_candidate_count: 10,
        selected_model_name: 'Whitened Full-Covariance SVGP',
        model_variant: 'whitened_svgp',
        fidelity: 'janus_mace_low',
        incumbent_target: -182.97277176606303,
        best_ranked_candidate_id: 'ga_pending_0003',
        highest_uncertainty_candidate_id: 'ga_pending_0008',
        top_acquisition_score: -2.6013536254217744,
        top_uncertainty_score: 3.1753833329084986,
        checkpoint_path: null,
        training_target_summary: null,
        predicted_target_summary: null,
        predicted_variance_summary: null,
        acquisition_score_summary: null,
        uncertainty_score_summary: { min: 2.0, max: 3.1753833329084986, mean: 2.7 },
        top_ranked_candidates: [],
        highest_uncertainty_candidates: [],
        predictions: [],
        feature_names: []
      }
    ]
  };

  const samplingFamilies = [
    {
      name: 'Basin hopping',
      status: 'Rust workflow + trace writer',
      trace: 'traces/walker_trace.csv',
      artifact: 'raw/basin_hopping_summary.json'
    },
    {
      name: 'Simulated annealing',
      status: 'Rust MC workflow + trace writer',
      trace: 'traces/simulated_annealing/*/mc_trace.csv',
      artifact: 'raw/simulated_annealing_summary.json'
    },
    {
      name: 'Energy lid / threshold sampling',
      status: 'Rust threshold workflow + per-structure traces',
      trace: 'traces/energy_lid/*/mc_trace.csv',
      artifact: 'raw/energy_lid_summary.json'
    }
  ];
  const topologyCapabilities = [
    'patina-dreadnaut already exists for topology identity and canonical graph work.',
    'Topology export and identity wiring already appears in the scientific and driver layers.',
    'The current pane renders the graph payload, near-cutoff evidence, and dreadnaut program for the selected run.'
  ];

  const programModes = [
    {
      name: 'uLab mode',
      posture: 'HPC orchestration',
      summary:
        'Defines and orchestrates HPC cluster simulations, scheduling run intent, payloads, and remote execution contracts.'
    },
    {
      name: 'Execution mode',
      posture: 'Single program entry',
      summary:
        'Defines one concrete executable pathway for a requested run, with a fixed engine contract and reproducible operator inputs.'
    },
    {
      name: 'App mode',
      posture: 'Local Tauri runtime',
      summary:
        'Runs simulations locally for now, streams GA progress into the UI, and entangles the important scientific records with the embedded database.'
    }
  ];
  const gaPopulationChart = {
    width: 760,
    height: 300,
    padLeft: 58,
    padRight: 20,
    padTop: 22,
    padBottom: 40
  };
  const maxReasonableAbsEnergyEv = 1e6;

  let overview = fallbackOverview;
  let database = fallbackDatabase;
  let dataFoundation = fallbackDataFoundation;
  let dataFlowLab = fallbackDataFlow;
  let autoemulateLab = fallbackAutoemulateLab;
  let catalog = fallbackCatalog;
  let loading = true;
  let syncing = false;
  let error = '';
  let backendMode = 'browser-preview';
  let activeLane = 'home';
  let selectedRunName =
    fallbackCatalog.runs.find((run) => run.runtime_engine === 'gulp')?.run_name ??
    fallbackCatalog.runs[0]?.run_name ??
    '';
  let lastSyncReport = null;
  let liveCatalogRefreshedAt = null;

  let slabFlowPath = `${workspaceRoot}/extra/inputs/mofs/zif8.cif`;
  let h = 1;
  let k = 0;
  let l = 0;
  let thickness = 12;
  let vacuum = 18;
  let repeatA = 1;
  let repeatB = 1;
  let dedupSlab = true;
  let slabFlowResult = null;

  let igaPopulation = 16;
  let igaGenerations = 12;
  let igaTemperature = 550;
  let igaMutationWeight = 0.5;
  let igaDiversityWeight = 0.5;
  let igaPreferredRunName = selectedRunName;
  let topologyLab = null;
  let topologyLoading = false;
  let topologyError = '';
  let topologyRequestedKey = '';
  let topologyHoveredNode = null;
  let topologyPinnedNodeIndex = null;
  let topologyStructurePath = '';
  let selectedRunDetail = null;
  let selectedRunDetailLoading = false;
  let selectedRunDetailError = '';
  let selectedRunDetailRequestedKey = '';
  let selectedAutoemulateExperimentKey =
    fallbackAutoemulateLab.experiments.find((experiment) => experiment.feature_family === 'pair_distance_signature')?.artifact_dir ??
    fallbackAutoemulateLab.experiments[0]?.artifact_dir ??
    '';
  let autoemulateDescriptorGroup = 'all';
  let autoemulateFeatureLimit = 24;
  let autoemulateSteeringMode = 'hybrid';
  let autoemulateBiasedCandidateIds = [];
  let autoemulateHoveredCandidateId = null;
  let selectedMaterialFlowId = 'structure_symmetry';
  let materialFlowCifPath = slabFlowPath;
  let materialFlowEngine = 'moyo';
  let materialFlowTolerance = 0.00001;
  let materialFlowNormalizationTarget = 'standardized';
  let perturbationSigma = 0.05;
  let perturbationCount = 8;
  let perturbationSeed = 7;
  let perturbationMaxDisplacement = 0.15;
  let perturbationMinDistance = 0.9;
  let perturbationMode = 's';
  let materialFlowResult = null;
  let materialFlowLoading = false;
  let materialFlowError = '';
  let reusableFlowStructure = null;
  let selectedStructureKey = 'best';
  let candidateStructurePreview = null;
  let candidateStructureLoading = false;
  let candidateStructureError = '';
  let candidateStructureRequestedKey = '';
  const materialFlowCatalog = [
    {
      id: 'structure_symmetry',
      label: 'Find structure symmetry',
      family: 'analysis',
      input: 'CIF framework',
      output: 'space group, operators, Wyckoff sites',
      engines: ['moyo', 'syva'],
      runnable: true
    },
    {
      id: 'normalize_framework',
      label: 'Normalise framework',
      family: 'transformation',
      input: 'CIF framework',
      output: 'standardized or primitive cell',
      engines: ['moyo'],
      runnable: true
    },
    {
      id: 'surface_generation',
      label: 'Generate slab',
      family: 'construction',
      input: 'CIF framework + Miller index',
      output: 'surface slab preview',
      engines: ['patina-surface'],
      runnable: true
    },
    {
      id: 'topology_identity',
      label: 'Topology identity',
      family: 'analysis',
      input: 'workspace run structure',
      output: 'canonical graph, hashkey, pair margins',
      engines: ['dreadnaut', 'patina-dreadnaut'],
      runnable: true
    },
    {
      id: 'global_search',
      label: 'Global search',
      family: 'search',
      input: 'seed structure + evaluator policy',
      output: 'ranked candidate population',
      engines: ['GA', 'BH', 'annealing', 'energy lid'],
      runnable: false
    },
    {
      id: 'perturb_cluster',
      label: 'Perturb cluster',
      family: 'variation',
      input: 'XYZ cluster',
      output: 'perturbed variants + fingerprint deltas',
      engines: ['patina-perturber'],
      runnable: true
    }
  ];

  function formatEnergy(value) {
    return value === null || value === undefined ? '-' : value.toFixed(6);
  }

  function formatPercent(numerator, denominator) {
    if (!denominator) return '-';
    return `${Math.round((numerator / denominator) * 100)}%`;
  }

  function formatDeltaEnergy(value) {
    if (value === null || value === undefined) return '-';
    const prefix = value > 0 ? '+' : '';
    return `${prefix}${value.toFixed(3)}`;
  }

  function escapeHtml(value) {
    return `${value}`
      .replaceAll('&', '&amp;')
      .replaceAll('<', '&lt;')
      .replaceAll('>', '&gt;')
      .replaceAll('"', '&quot;')
      .replaceAll("'", '&#39;');
  }

  function formatFormulaMarkup(formula) {
    if (!formula || formula === '-') return '-';
    return escapeHtml(formula).replace(/([A-Za-z)])(\d+(?:\.\d+)?)/g, '$1<sub>$2</sub>');
  }

  function speciesColor(species) {
    const palette = {
      Ti: '#2f7f8f',
      N: '#d36e54',
      O: '#c39a36',
      Mg: '#5f84d3',
      Ca: '#6e5bd3'
    };
    return palette[species] || '#54626a';
  }

  function originColor(origin) {
    const palette = {
      REPOPR: '#0f5f70',
      MUTATE: '#c86b3c',
      CROSS: '#6c5ab5',
      CROSSO: '#6c5ab5',
      MUTCRS: '#8a4f9f',
      ELITE: '#2c8b57',
      SEED: '#7d8890',
      RANDOM: '#7d8890'
    };
    return palette[origin] || '#54626a';
  }

  function preferredDefaultRunName(runs) {
    return runs.find((run) => run.runtime_engine?.toLowerCase() === 'gulp')?.run_name ?? runs[0]?.run_name ?? '';
  }

  function engineLabel(run) {
    return run?.runtime_engine ? run.runtime_engine.toUpperCase() : run?.backend || '-';
  }

  function formatTimestamp(unixMs) {
    if (!unixMs) return 'awaiting live state';
    return new Date(unixMs).toLocaleString();
  }

  function shortHashkey(hashkey) {
    if (!hashkey) return 'pending';
    return hashkey.length > 44 ? `${hashkey.slice(0, 44)}...` : hashkey;
  }

  function sumBy(rows, field) {
    return rows.reduce((sum, row) => sum + (row?.[field] ?? 0), 0);
  }

  function generationObservations(run) {
    const observations = run?.ga_generations ?? [];
    if (observations.length) return observations;

    return (run?.ga_live?.history ?? []).map((point) => ({
      generation: point.generation,
      phase: 'live',
      request_count: 0,
      success_count: 0,
      failure_count: 0,
      failure_kind_counts: {},
      converged_count: point.converged_count ?? 0,
      population_size: point.population_size ?? 0,
      valid_population_size: point.population_size ?? 0,
      duplicate_count: 0,
      duplicate_hashkey_count: 0,
      duplicate_pmoi_count: 0,
      duplicate_energy_tol_count: 0,
      repopulated_count: point.repopulation_count ?? 0,
      best_energy: point.best_energy,
      mean_energy: point.mean_energy,
      worst_energy: point.worst_energy
    }));
  }

  function latestObservationForRun(run) {
    const observations = generationObservations(run);
    return observations.at(-1) ?? null;
  }

  function firstEnergyObservation(observations) {
    return observations.find((row) => row.best_energy !== null && row.best_energy !== undefined) ?? null;
  }

  function buildGaHealthCards(observations, live, topCandidates = []) {
    const latest = observations.at(-1) ?? null;
    const firstEnergy = firstEnergyObservation(observations);
    const incumbentBest =
      topCandidates.find((candidate) => candidate.energy !== null && candidate.energy !== undefined)?.energy ??
      live?.best_energy ??
      latest?.best_energy ??
      null;
    const energyGain =
      firstEnergy?.best_energy !== null && firstEnergy?.best_energy !== undefined && incumbentBest !== null
        ? firstEnergy.best_energy - incumbentBest
        : null;
    const totalRequests = sumBy(observations, 'request_count');
    const totalSuccess = sumBy(observations, 'success_count');
    const totalConverged = sumBy(observations, 'converged_count');
    const totalDuplicates = sumBy(observations, 'duplicate_count');
    const totalRepopulated = sumBy(observations, 'repopulated_count');
    const latestPopulation = latest?.population_size ?? live?.population_size ?? 0;
    const latestValid = latest?.valid_population_size ?? live?.population_size ?? 0;

    return [
      { label: 'Best energy', value: formatEnergy(incumbentBest), detail: 'eV, incumbent candidate' },
      { label: 'Delta best', value: formatDeltaEnergy(energyGain), detail: 'eV improvement from first observed generation' },
      { label: 'Success rate', value: formatPercent(totalSuccess, totalRequests), detail: `${totalSuccess}/${totalRequests || 0} accepted evaluator calls` },
      { label: 'Valid population', value: formatPercent(latestValid, latestPopulation), detail: `${latestValid}/${latestPopulation || 0} latest population members` },
      { label: 'Converged', value: formatPercent(totalConverged, totalSuccess), detail: `${totalConverged}/${totalSuccess || 0} successful calls` },
      { label: 'Duplicate / repop', value: `${totalDuplicates} / ${totalRepopulated}`, detail: 'diversity pressure and recovery attempts' }
    ];
  }

  function topCandidatesForRun(run) {
    if (run?.top_candidates?.length) return run.top_candidates.slice(0, 5);

    return (run?.ga_live?.population ?? [])
      .filter((member) => member.energy !== null && member.energy !== undefined)
      .sort((left, right) => left.energy - right.energy)
      .slice(0, 5)
      .map((member, index) => ({
        unique_rank: index + 1,
        population_rank: index,
        origin: member.origin,
        occurrences: member.occurrences,
        energy: member.energy,
        converged: member.converged,
        label: member.label,
        structure_path: null,
        canonical_hashkey: member.canonical_hashkey
      }));
  }

  function statusClass(status) {
    if (status === 'present' || status === 'ready') return 'status-present';
    if (status === 'missing' || status === 'partial' || status === 'error') return 'status-missing';
    return 'status-neutral';
  }

  function failureKindSummary(observation) {
    const entries = Object.entries(observation?.failure_kind_counts ?? {});
    if (!entries.length) return 'none';
    return entries.map(([kind, count]) => `${kind}: ${count}`).join(', ');
  }

  function isReasonableEnergy(value) {
    return Number.isFinite(value) && Math.abs(value) < maxReasonableAbsEnergyEv;
  }

  function structureOptionKey(candidate) {
    return candidate?.label ? `candidate:${candidate.label}` : 'best';
  }

  function structureOptionsForRun(run) {
    const options = [{ key: 'best', label: 'Current best structure', path: null }];
    for (const candidate of topCandidatesForRun(run)) {
      if (!candidate.structure_path) continue;
      options.push({
        key: structureOptionKey(candidate),
        label: `#${candidate.unique_rank} ${candidate.label}`,
        path: candidate.structure_path
      });
    }
    return options;
  }

  function distance(left, right) {
    const dx = left[0] - right[0];
    const dy = left[1] - right[1];
    const dz = left[2] - right[2];
    return Math.sqrt(dx * dx + dy * dy + dz * dz);
  }

  function dot(left, right) {
    return left[0] * right[0] + left[1] * right[1] + left[2] * right[2];
  }

  function cross(left, right) {
    return [
      left[1] * right[2] - left[2] * right[1],
      left[2] * right[0] - left[0] * right[2],
      left[0] * right[1] - left[1] * right[0]
    ];
  }

  function normalize(vector) {
    const length = Math.hypot(vector[0], vector[1], vector[2]);
    return length > 1e-9 ? vector.map((value) => value / length) : [1, 0, 0];
  }

  function orthogonalize(vector, basis) {
    return [
      vector[0] - dot(vector, basis) * basis[0],
      vector[1] - dot(vector, basis) * basis[1],
      vector[2] - dot(vector, basis) * basis[2]
    ];
  }

  function multiplyMatrixVector(matrix, vector) {
    return matrix.map((row) => dot(row, vector));
  }

  function principalAxis(matrix, seed, orthogonalBasis = null) {
    let vector = normalize(seed);
    if (orthogonalBasis) {
      vector = normalize(orthogonalize(vector, orthogonalBasis));
    }
    for (let iteration = 0; iteration < 18; iteration += 1) {
      let next = multiplyMatrixVector(matrix, vector);
      if (orthogonalBasis) {
        next = orthogonalize(next, orthogonalBasis);
      }
      const length = Math.hypot(next[0], next[1], next[2]);
      if (length <= 1e-9) {
        break;
      }
      vector = next.map((value) => value / length);
    }
    return vector;
  }

  function projectToBestFitPlane(sites, options = {}) {
    if (!sites.length) return null;

    const width = options.width ?? 320;
    const height = options.height ?? 320;
    const padding = options.padding ?? 26;
    const coords = sites.map((site) => site.xyz ?? [0, 0, 0]);
    const center = [0, 1, 2].map(
      (axis) => coords.reduce((sum, point) => sum + point[axis], 0) / coords.length
    );
    const centered = coords.map((point) => point.map((value, axis) => value - center[axis]));
    const covariance = [
      [0, 0, 0],
      [0, 0, 0],
      [0, 0, 0]
    ];

    for (const point of centered) {
      for (let row = 0; row < 3; row += 1) {
        for (let col = 0; col < 3; col += 1) {
          covariance[row][col] += point[row] * point[col];
        }
      }
    }

    const axis1 = principalAxis(covariance, [1, 0.41, 0.23]);
    let axis2 = principalAxis(covariance, [0.19, 1, 0.53], axis1);
    if (Math.hypot(axis2[0], axis2[1], axis2[2]) <= 1e-9) {
      axis2 = normalize(orthogonalize([0, 0, 1], axis1));
    }
    let normal = normalize(cross(axis1, axis2));
    if (Math.hypot(normal[0], normal[1], normal[2]) <= 1e-9) {
      normal = [0, 0, 1];
    }

    const rawPoints = centered.map((point) => ({
      u: dot(point, axis1),
      v: dot(point, axis2),
      depth: dot(point, normal)
    }));
    const us = rawPoints.map((point) => point.u);
    const vs = rawPoints.map((point) => point.v);
    const depths = rawPoints.map((point) => point.depth);
    const minU = Math.min(...us);
    const maxU = Math.max(...us);
    const minV = Math.min(...vs);
    const maxV = Math.max(...vs);
    const minDepth = Math.min(...depths);
    const maxDepth = Math.max(...depths);
    const spanU = Math.max(maxU - minU, 1);
    const spanV = Math.max(maxV - minV, 1);
    const spanDepth = Math.max(maxDepth - minDepth, 1);

    return {
      width,
      height,
      method: 'best-fit-plane',
      atoms: sites
        .map((site, index) => {
          const species = site.species?.[0]?.element || 'X';
          const raw = rawPoints[index];
          const depthRatio = (raw.depth - minDepth) / spanDepth;
          return {
            index,
            species,
            label: site.label || `${species}${index + 1}`,
            x: padding + ((raw.u - minU) / spanU) * (width - padding * 2),
            y: height - padding - ((raw.v - minV) / spanV) * (height - padding * 2),
            depth: raw.depth,
            radius: 7.5 + depthRatio * 5.5,
            color: speciesColor(species),
            opacity: 0.52 + depthRatio * 0.36
          };
        })
        .sort((left, right) => left.depth - right.depth)
    };
  }

  function buildStructureProjection(structure, options = {}) {
    const sites = structure?.sites || [];
    if (!sites.length) return null;
    return projectToBestFitPlane(sites, options);
  }

  function buildPopulationPlot(population) {
    if (!population?.length) return null;

    const filtered = population.filter((member) => isReasonableEnergy(member.energy));
    if (!filtered.length) return null;

    const ordered = [...filtered].sort((left, right) => {
      const leftEnergy = left.energy ?? Number.POSITIVE_INFINITY;
      const rightEnergy = right.energy ?? Number.POSITIVE_INFINITY;
      return leftEnergy - rightEnergy;
    });
    const energyValues = ordered
      .map((member) => member.energy)
      .filter((value) => value !== null && value !== undefined);
    if (!energyValues.length) return null;

    const width = gaPopulationChart.width;
    const height = gaPopulationChart.height;
    const plotWidth = width - gaPopulationChart.padLeft - gaPopulationChart.padRight;
    const plotHeight = height - gaPopulationChart.padTop - gaPopulationChart.padBottom;
    const minEnergy = Math.min(...energyValues);
    const maxEnergy = Math.max(...energyValues);
    const spanEnergy = Math.max(maxEnergy - minEnergy, 1);
    const ticks = Array.from({ length: 4 }, (_, tickIndex) => {
      const ratio = tickIndex / 3;
      const value = maxEnergy - ratio * spanEnergy;
      return {
        value,
        y: gaPopulationChart.padTop + ratio * plotHeight
      };
    });

    return {
      width,
      height,
      axisY: height - gaPopulationChart.padBottom,
      ticks,
      points: ordered.map((member, index) => {
        const x =
          gaPopulationChart.padLeft +
          (ordered.length === 1 ? plotWidth / 2 : (index / (ordered.length - 1)) * plotWidth);
        const energy = member.energy ?? maxEnergy;
        return {
          ...member,
          rank: index + 1,
          x,
          y: gaPopulationChart.padTop + ((maxEnergy - energy) / spanEnergy) * plotHeight,
          color: originColor(member.origin)
        };
      })
    };
  }

  function buildGaTrendSeries(history) {
    const points = (history ?? []).filter(
      (point) =>
        point.best_energy != null ||
        point.mean_energy != null ||
        point.worst_energy != null
    );
    if (!points.length) return [];

    return [
      {
        id: 'best',
        label: 'Best',
        x: points.map((point) => point.generation),
        y: points.map((point) => point.best_energy ?? point.mean_energy ?? point.worst_energy),
        markers: 'line+points',
        line_style: { stroke: '#0f5f70', stroke_width: 3 },
        point_style: { fill: '#0f5f70', stroke: '#0f5f70', radius: 5 }
      },
      {
        id: 'mean',
        label: 'Mean',
        x: points.map((point) => point.generation),
        y: points.map((point) => point.mean_energy ?? point.best_energy ?? point.worst_energy),
        markers: 'line+points',
        line_style: { stroke: '#c86b3c', stroke_width: 3, line_dash: '5 4' },
        point_style: { fill: '#c86b3c', stroke: '#c86b3c', radius: 4.5 }
      },
      {
        id: 'worst',
        label: 'Worst',
        x: points.map((point) => point.generation),
        y: points.map((point) => point.worst_energy ?? point.mean_energy ?? point.best_energy),
        markers: 'line+points',
        line_style: { stroke: '#7d8890', stroke_width: 2.5, line_dash: '2 5' },
        point_style: { fill: '#7d8890', stroke: '#7d8890', radius: 4 }
      }
    ];
  }

  function samplingSeedEnergy(run) {
    return run?.best_structure?.energy ?? -183.0;
  }

  function buildSamplingExperiments(run) {
    const base = samplingSeedEnergy(run);
    return [
      {
        id: 'bh',
        method: 'Basin hopping',
        artifact: 'traces/walker_trace.csv',
        summary: 'raw/basin_hopping_summary.json',
        provenance: 'Rust BasinHoppingWorkflowService + LocalBasinHoppingArtifactSink',
        rows: [
          { step: 0, energy: base + 1.18, best_energy: base + 1.18, temperature: 625, accepted: true, threshold: null },
          { step: 1, energy: base + 0.72, best_energy: base + 0.72, temperature: 625, accepted: true, threshold: null },
          { step: 2, energy: base + 0.93, best_energy: base + 0.72, temperature: 625, accepted: false, threshold: null },
          { step: 3, energy: base + 0.24, best_energy: base + 0.24, temperature: 625, accepted: true, threshold: null },
          { step: 4, energy: base + 0.41, best_energy: base + 0.24, temperature: 625, accepted: false, threshold: null },
          { step: 5, energy: base - 0.06, best_energy: base - 0.06, temperature: 625, accepted: true, threshold: null }
        ]
      },
      {
        id: 'anneal',
        method: 'Simulated annealing',
        artifact: 'traces/simulated_annealing/*/mc_trace.csv',
        summary: 'raw/simulated_annealing_summary.json',
        provenance: 'EnergyLidWorkflowService::execute_simulated_annealing',
        rows: [
          { step: 0, energy: base + 1.05, best_energy: base + 1.05, temperature: 900, accepted: true, threshold: null },
          { step: 1, energy: base + 0.82, best_energy: base + 0.82, temperature: 720, accepted: true, threshold: null },
          { step: 2, energy: base + 0.96, best_energy: base + 0.82, temperature: 540, accepted: false, threshold: null },
          { step: 3, energy: base + 0.38, best_energy: base + 0.38, temperature: 360, accepted: true, threshold: null },
          { step: 4, energy: base + 0.44, best_energy: base + 0.38, temperature: 180, accepted: false, threshold: null },
          { step: 5, energy: base + 0.08, best_energy: base + 0.08, temperature: 90, accepted: true, threshold: null }
        ]
      },
      {
        id: 'lid',
        method: 'Energy lid',
        artifact: 'traces/energy_lid/*/mc_trace.csv',
        summary: 'raw/energy_lid_summary.json',
        provenance: 'EnergyLidWorkflowService::execute_energy_lid',
        rows: [
          { step: 0, energy: base + 0.12, best_energy: base + 0.12, temperature: 0, accepted: true, threshold: base + 0.20 },
          { step: 1, energy: base + 0.31, best_energy: base + 0.12, temperature: 0, accepted: false, threshold: base + 0.20 },
          { step: 2, energy: base + 0.18, best_energy: base + 0.12, temperature: 0, accepted: true, threshold: base + 0.35 },
          { step: 3, energy: base + 0.42, best_energy: base + 0.12, temperature: 0, accepted: false, threshold: base + 0.35 },
          { step: 4, energy: base + 0.27, best_energy: base + 0.12, temperature: 0, accepted: true, threshold: base + 0.50 },
          { step: 5, energy: base + 0.09, best_energy: base + 0.09, temperature: 0, accepted: true, threshold: base + 0.50 }
        ]
      }
    ];
  }

  function buildSamplingTrendSeries(experiments) {
    return experiments.map((experiment) => {
      const color = experiment.id === 'bh' ? '#0f5f70' : experiment.id === 'anneal' ? '#c86b3c' : '#6f7f43';
      return {
        id: experiment.id,
        label: experiment.method,
        x: experiment.rows.map((row) => row.step),
        y: experiment.rows.map((row) => row.best_energy),
        markers: 'line+points',
        line_style: { stroke: color, stroke_width: 3 },
        point_style: { fill: color, stroke: color, radius: 5 }
      };
    });
  }

  function buildSamplingAcceptanceRows(experiments) {
    return experiments.map((experiment) => {
      const accepted = experiment.rows.filter((row) => row.accepted).length;
      return {
        method: experiment.method,
        accepted,
        total: experiment.rows.length,
        rate: experiment.rows.length ? accepted / experiment.rows.length : 0
      };
    });
  }

  function buildAutoemulatePredictionSeries(experiment) {
    const rows = experiment?.predictions ?? [];
    if (!rows.length) return [];
    return [
      {
        id: 'predicted',
        label: 'Predicted energy',
        x: rows.map((row) => row.rank),
        y: rows.map((row) => row.predicted_mean),
        markers: 'line+points',
        line_style: { stroke: '#0f5f70', stroke_width: 3 },
        point_style: { fill: '#0f5f70', stroke: '#0f5f70', radius: 5 }
      }
    ];
  }

  function buildAutoemulateAcquisitionSeries(experiment) {
    const rows = experiment?.predictions ?? [];
    return [
      {
        id: 'acquisition',
        label: 'Acquisition',
        x: rows.map((row) => row.rank),
        y: rows.map((row) => row.acquisition_score),
        markers: 'line+points',
        line_style: { stroke: '#c86b3c', stroke_width: 3 },
        point_style: { fill: '#c86b3c', stroke: '#c86b3c', radius: 5 }
      },
      {
        id: 'uncertainty',
        label: 'Uncertainty',
        x: rows.map((row) => row.rank),
        y: rows.map((row) => row.uncertainty_score),
        markers: 'line+points',
        line_style: { stroke: '#0f5f70', stroke_width: 3, line_dash: '5 4' },
        point_style: { fill: '#0f5f70', stroke: '#0f5f70', radius: 5 }
      }
    ];
  }

  function seriesRows(series) {
    return (series?.x ?? []).map((xValue, index) => ({
      x: xValue,
      y: series.y?.[index],
      label: series.label,
      color: series.line_style?.stroke ?? '#0f5f70'
    }));
  }

  function seriesStroke(series) {
    return series?.line_style?.stroke ?? '#0f5f70';
  }

  function seriesDash(series) {
    return series?.line_style?.line_dash ?? null;
  }

  function autoemulateExperimentKey(experiment) {
    return experiment?.artifact_dir || experiment?.campaign_id || '';
  }

  function autoemulateDisplayName(experiment) {
    return experiment?.feature_family?.replaceAll('_', ' ') || experiment?.campaign_id || 'experiment';
  }

  function selectedAutoemulateExperimentFromLab(lab, key) {
    return (
      lab.experiments.find((experiment) => autoemulateExperimentKey(experiment) === key) ??
      lab.experiments.find((experiment) => experiment.feature_family === 'pair_distance_signature') ??
      lab.experiments[0] ??
      null
    );
  }

  function scalarMean(summary) {
    return summary?.mean ?? null;
  }

  function buildAutoemulateExperimentSeries(experiments, field, label, color) {
    const rows = experiments.filter((experiment) => Number.isFinite(experiment?.[field]));
    return [
      {
        id: field,
        label,
        x: rows.map((_, index) => index + 1),
        y: rows.map((experiment) => experiment[field]),
        markers: 'line+points',
        line_style: { stroke: color, stroke_width: 3 },
        point_style: { fill: color, stroke: color, radius: 5 }
      }
    ];
  }

  function buildSveltePlotExperimentRows(experiments) {
    return experiments.map((experiment, index) => ({
      index: index + 1,
      feature_family: experiment.feature_family,
      feature_count: experiment.feature_count,
      mean_uncertainty: scalarMean(experiment.uncertainty_score_summary),
      mean_variance: scalarMean(experiment.predicted_variance_summary),
      top_acquisition: experiment.top_acquisition_score
    }));
  }

  function buildAutoemulateHeatmap(experiments) {
    const columns = [
      { key: 'feature_count', label: 'Features', value: (experiment) => experiment.feature_count },
      { key: 'mean_prediction', label: 'Predicted', value: (experiment) => scalarMean(experiment.predicted_target_summary) },
      { key: 'mean_variance', label: 'Variance', value: (experiment) => scalarMean(experiment.predicted_variance_summary) },
      { key: 'mean_uncertainty', label: 'Uncertainty', value: (experiment) => scalarMean(experiment.uncertainty_score_summary) },
      { key: 'top_acquisition', label: 'Top acq.', value: (experiment) => experiment.top_acquisition_score }
    ];
    const rows = experiments.map((experiment) => ({
      key: autoemulateExperimentKey(experiment),
      label: experiment.feature_family,
      cells: columns.map((column) => ({
        key: column.key,
        label: column.label,
        raw: column.value(experiment)
      }))
    }));
    const ranges = columns.map((column, columnIndex) => {
      const values = rows
        .map((row) => row.cells[columnIndex].raw)
        .filter((value) => Number.isFinite(value));
      return {
        key: column.key,
        label: column.label,
        min: values.length ? Math.min(...values) : 0,
        max: values.length ? Math.max(...values) : 1
      };
    });
    return { columns, rows, ranges };
  }

  function heatmapCellStyle(cell, range) {
    if (!Number.isFinite(cell.raw)) {
      return 'background:rgba(17,33,40,0.04);color:#7d8890;';
    }
    const span = Math.max(range.max - range.min, 1e-9);
    const ratio = (cell.raw - range.min) / span;
    const lightness = 92 - ratio * 38;
    return `background:hsl(190 55% ${lightness}%);color:#112128;`;
  }

  function descriptorGroupForFeature(name) {
    if (name.startsWith('pair_') || name.includes('distance')) return 'pair';
    if (name.startsWith('species_') || name.includes('fraction')) return 'composition';
    if (name.startsWith('span_') || name.includes('radius') || name.includes('neighbor')) return 'geometry';
    if (name.includes('soap')) return 'soap';
    return 'global';
  }

  function descriptorOptions(experiment) {
    const groups = new Set((experiment?.feature_names ?? []).map(descriptorGroupForFeature));
    return ['all', ...Array.from(groups).sort()];
  }

  function selectedDescriptorIndices(experiment, group, limit) {
    const featureNames = experiment?.feature_names ?? [];
    const all = featureNames
      .map((name, index) => ({ name, index, group: descriptorGroupForFeature(name) }))
      .filter((item) => group === 'all' || item.group === group);
    return all.slice(0, Math.max(2, Number(limit) || 2));
  }

  function finiteFeatureValue(row, index) {
    const value = row?.features?.[index];
    return Number.isFinite(value) ? value : 0;
  }

  function scaleLinear(value, min, max, low, high) {
    const span = Math.max(max - min, 1e-9);
    return low + ((value - min) / span) * (high - low);
  }

  function dotProduct(left, right) {
    return left.reduce((sum, value, index) => sum + value * (right[index] ?? 0), 0);
  }

  function vectorNorm(vector) {
    return Math.sqrt(vector.reduce((sum, value) => sum + value * value, 0));
  }

  function normalizeVector(vector) {
    const norm = vectorNorm(vector);
    if (!Number.isFinite(norm) || norm < 1e-12) return vector.map(() => 0);
    return vector.map((value) => value / norm);
  }

  function matrixVectorProduct(matrix, vector) {
    return matrix.map((row) => dotProduct(row, vector));
  }

  function covarianceMatrix(matrix) {
    const columnCount = matrix[0]?.length ?? 0;
    const divisor = Math.max(matrix.length - 1, 1);
    return Array.from({ length: columnCount }, (_, rowIndex) =>
      Array.from({ length: columnCount }, (_, columnIndex) =>
        matrix.reduce((sum, row) => sum + row[rowIndex] * row[columnIndex], 0) / divisor
      )
    );
  }

  function powerIteration(matrix, blockedVector = null) {
    const size = matrix.length;
    if (!size) return { vector: [], value: 0 };
    let vector = normalizeVector(
      Array.from({ length: size }, (_, index) => {
        const base = index % 2 === 0 ? 1 : -0.5;
        return blockedVector ? base - (blockedVector[index] ?? 0) : base;
      })
    );

    for (let iteration = 0; iteration < 80; iteration += 1) {
      let next = matrixVectorProduct(matrix, vector);
      if (blockedVector) {
        const overlap = dotProduct(next, blockedVector);
        next = next.map((value, index) => value - overlap * (blockedVector[index] ?? 0));
      }
      const norm = vectorNorm(next);
      if (!Number.isFinite(norm) || norm < 1e-12) break;
      vector = next.map((value) => value / norm);
    }

    const projected = matrixVectorProduct(matrix, vector);
    return { vector, value: dotProduct(vector, projected) };
  }

  function standardizeFeatureMatrix(rows, indices) {
    const activeFeatures = indices.filter((feature) =>
      rows.some((row) => Number.isFinite(row?.features?.[feature.index]))
    );
    if (!rows.length || !activeFeatures.length) return null;

    const stats = activeFeatures.map((feature) => {
      const values = rows.map((row) => finiteFeatureValue(row, feature.index));
      const mean = values.reduce((sum, value) => sum + value, 0) / values.length;
      const variance =
        values.reduce((sum, value) => sum + (value - mean) ** 2, 0) / Math.max(values.length - 1, 1);
      return { mean, std: Math.sqrt(Math.max(variance, 1e-12)) };
    });

    return {
      activeFeatures,
      matrix: rows.map((row) =>
        activeFeatures.map((feature, index) =>
          (finiteFeatureValue(row, feature.index) - stats[index].mean) / stats[index].std
        )
      )
    };
  }

  function descriptorHeatColor(value) {
    if (!Number.isFinite(value)) return '#edf2f4';
    const clamped = Math.max(0, Math.min(1, value));
    const hue = 196 - clamped * 36;
    const saturation = 66 - clamped * 8;
    const lightness = 91 - clamped * 41;
    return `hsl(${hue} ${saturation}% ${lightness}%)`;
  }

  function buildDescriptorProjection(experiment, indices) {
    const rows = experiment?.predictions ?? [];
    if (!rows.length || !indices.length) return null;

    const standardized = standardizeFeatureMatrix(rows, indices);
    if (!standardized) return null;
    const covariance = covarianceMatrix(standardized.matrix);
    const first = powerIteration(covariance);
    const second = powerIteration(covariance, first.vector);
    const trace = Math.max(covariance.reduce((sum, row, index) => sum + (row[index] ?? 0), 0), 1e-12);
    const acq = rows.map((row) => row.acquisition_score).filter(Number.isFinite);
    const minAcq = acq.length ? Math.min(...acq) : 0;
    const maxAcq = acq.length ? Math.max(...acq) : 1;
    const maxUncertainty = Math.max(...rows.map((item) => item.uncertainty_score).filter(Number.isFinite), 1);

    return {
      xLabel: `PC1 (${Math.round((first.value / trace) * 100)}%)`,
      yLabel: `PC2 (${Math.round((Math.max(second.value, 0) / trace) * 100)}%)`,
      featureCount: standardized.activeFeatures.length,
      points: rows.map((row, rowIndex) => {
        const vector = standardized.matrix[rowIndex];
        const acquisitionRatio = scaleLinear(row.acquisition_score, minAcq, maxAcq, 0, 1);
        return {
          ...row,
          pc1: dotProduct(vector, first.vector),
          pc2: dotProduct(vector, second.vector),
          radius: 5 + scaleLinear(row.uncertainty_score, 0, maxUncertainty, 0, 8),
          acquisitionRatio,
          color: autoemulateBiasedCandidateIds.includes(row.candidate_id)
            ? '#c86b3c'
            : `hsl(${190 - acquisitionRatio * 55} 68% 42%)`,
          stroke: autoemulateHoveredCandidateId === row.candidate_id ? '#112128' : '#ffffff',
          strokeWidth: autoemulateBiasedCandidateIds.includes(row.candidate_id) ? 3 : 1.8
        };
      })
    };
  }

  function selectedMaterialFlow() {
    return materialFlowCatalog.find((flow) => flow.id === selectedMaterialFlowId) ?? materialFlowCatalog[0];
  }

  function toggleAutoemulateBias(candidateId) {
    if (!candidateId) return;
    autoemulateBiasedCandidateIds = autoemulateBiasedCandidateIds.includes(candidateId)
      ? autoemulateBiasedCandidateIds.filter((id) => id !== candidateId)
      : [...autoemulateBiasedCandidateIds, candidateId];
  }

  function normalizeField(rows, field, value) {
    const values = rows.map((row) => row[field]).filter(Number.isFinite);
    if (!values.length || !Number.isFinite(value)) return 0;
    return (value - Math.min(...values)) / Math.max(Math.max(...values) - Math.min(...values), 1e-9);
  }

  function buildSteeredQueue(experiment, mode, biasedIds) {
    const rows = [...(experiment?.predictions ?? [])];
    return rows
      .map((row) => {
        const acquisition = normalizeField(rows, 'acquisition_score', row.acquisition_score);
        const uncertainty = normalizeField(rows, 'uncertainty_score', row.uncertainty_score);
        const userBias = biasedIds.includes(row.candidate_id) ? 1 : 0;
        const score =
          mode === 'unsupervised'
            ? acquisition + 0.25 * uncertainty
            : mode === 'user'
              ? userBias + 0.15 * acquisition
              : acquisition + 0.35 * uncertainty + 0.65 * userBias;
        return { ...row, steering_score: score, user_bias: userBias };
      })
      .sort((left, right) => right.steering_score - left.steering_score);
  }

  function buildCandidateFeatureHeatmap(experiment, indices, queue) {
    const rows = queue.slice(0, 8);
    const columns = indices.slice(0, 12);
    const ranges = columns.map((feature) => {
      const values = rows.map((row) => finiteFeatureValue(row, feature.index));
      return { min: Math.min(...values), max: Math.max(...values) };
    });
    const labeledRows = rows.map((row, rowIndex) => ({
      candidate_id: row.candidate_id,
      label: row.label,
      rowLabel: `${rowIndex + 1}. ${row.label || row.candidate_id}`,
      cells: columns.map((feature, columnIndex) => {
        const raw = finiteFeatureValue(row, feature.index);
        const range = ranges[columnIndex];
        const scaled = Number.isFinite(raw)
          ? (raw - range.min) / Math.max(range.max - range.min, 1e-9)
          : null;
        return {
          raw,
          scaled,
          feature: feature.name,
          featureLabel: feature.name.length > 18 ? `${feature.name.slice(0, 15)}...` : feature.name,
          candidate: `${rowIndex + 1}. ${row.label || row.candidate_id}`,
          candidate_id: row.candidate_id,
          color: descriptorHeatColor(scaled)
        };
      })
    }));
    return {
      columns,
      rows: labeledRows,
      cells: labeledRows.flatMap((row) => row.cells),
      ranges
    };
  }

  function edgeKey(left, right) {
    return left < right ? `${left}:${right}` : `${right}:${left}`;
  }

  function buildTopologyProjection(structure, graph, focusedNodeIndex, options = {}) {
    const projection = buildStructureProjection(structure, options);
    if (!projection || !graph) return null;

    const nodesByIndex = new Map(
      graph.nodes.map((node) => [
        node.index,
        {
          ...node,
          ...(projection.atoms.find((atom) => atom.index === node.index) || {}),
          color: speciesColor(node.species)
        }
      ])
    );

    const focusedEdges = graph.edges
      .filter(([left, right]) => {
        if (focusedNodeIndex === null || focusedNodeIndex === undefined) return false;
        return left === focusedNodeIndex || right === focusedNodeIndex;
      })
      .map(([left, right]) => ({
        left: nodesByIndex.get(left),
        right: nodesByIndex.get(right)
      }))
      .filter((edge) => edge.left && edge.right);

    return {
      width: projection.width,
      height: projection.height,
      method: projection.method,
      nodes: Array.from(nodesByIndex.values()).sort((left, right) => (left.depth ?? 0) - (right.depth ?? 0)),
      focusedEdges
    };
  }

  function buildTopologyMatrix(graph, focusedNodeIndex) {
    if (!graph?.nodes?.length) return null;

    const order = graph.color_partitions.flatMap((partition) =>
      [...partition].sort((left, right) => {
        const leftNode = graph.nodes.find((node) => node.index === left);
        const rightNode = graph.nodes.find((node) => node.index === right);
        const degreeDelta = (rightNode?.degree || 0) - (leftNode?.degree || 0);
        if (degreeDelta !== 0) return degreeDelta;
        return left - right;
      })
    );

    const edgeSet = new Set(graph.edges.map(([left, right]) => edgeKey(left, right)));
    const cellSize = Math.max(16, Math.min(26, 320 / Math.max(order.length, 1)));
    const inset = 54;
    const cells = order.flatMap((rowNode, rowIndex) =>
      order.map((colNode, colIndex) => {
        const active = edgeSet.has(edgeKey(rowNode, colNode));
        const diagonal = rowNode === colNode;
        return {
          rowNode,
          colNode,
          x: inset + colIndex * cellSize,
          y: inset + rowIndex * cellSize,
          active,
          diagonal,
          emphasized:
            focusedNodeIndex !== null &&
            focusedNodeIndex !== undefined &&
            (rowNode === focusedNodeIndex || colNode === focusedNodeIndex)
        };
      })
    );

    return {
      width: inset + order.length * cellSize + 16,
      height: inset + order.length * cellSize + 16,
      cellSize,
      inset,
      order,
      cells
    };
  }

  function buildFallbackTopology(run) {
    const sites = run?.structure_preview?.sites || [];
    const nodes = sites.map((site, index) => ({
      index,
      species: site.species[0]?.element || 'X',
      degree: 0
    }));
    const edges = [];
    const radius = 2.85;
    for (let i = 0; i < sites.length; i += 1) {
      for (let j = i + 1; j < sites.length; j += 1) {
        const d = distance(sites[i].xyz, sites[j].xyz);
        if (d <= radius) {
          edges.push([i, j]);
          nodes[i].degree += 1;
          nodes[j].degree += 1;
        }
      }
    }
    const partitions = [];
    for (const species of Array.from(new Set(nodes.map((node) => node.species))).sort()) {
      partitions.push(
        nodes.filter((node) => node.species === species).map((node) => node.index)
      );
    }
    const degreeHistogram = {};
    for (const node of nodes) {
      degreeHistogram[node.species] ||= {};
      degreeHistogram[node.species][node.degree] = (degreeHistogram[node.species][node.degree] || 0) + 1;
    }
    const nearPairs = [];
    for (let i = 0; i < sites.length; i += 1) {
      for (let j = i + 1; j < sites.length; j += 1) {
        const d = distance(sites[i].xyz, sites[j].xyz);
        nearPairs.push({
          left: i,
          right: j,
          left_species: sites[i].species[0]?.element || 'X',
          right_species: sites[j].species[0]?.element || 'X',
          distance: d,
          margin: radius - d,
          is_edge: d <= radius
        });
      }
    }
    nearPairs.sort((left, right) => Math.abs(left.margin) - Math.abs(right.margin));
    return {
      run_name: run?.run_name,
      source_path: null,
      source_preview: run?.structure_preview ?? null,
      radius,
      radius_mode: 'preview',
      radius_const: radius,
      graph: {
        nodes,
        edges,
        color_partitions: partitions
      },
      summary: {
        node_count: nodes.length,
        edge_count: edges.length,
        connected_components: Math.max(1, partitions.length),
        degree_histogram_by_species: degreeHistogram
      },
      graph_text: 'Fallback browser preview. Open in Tauri for bundled dreadnaut canonicalization.',
      canonical_hashkey: null,
      hashkey_error: 'Canonical dreadnaut hashkey is only available in the desktop runtime.',
      near_cutoff_pairs: nearPairs.slice(0, 18)
    };
  }

  function buildTopologyIdentityLayers(topology) {
    if (!topology) return [];
    const partitionCount = topology.graph?.color_partitions?.length ?? 0;
    const nearCritical = topology.near_cutoff_pairs?.filter((pair) => Math.abs(pair.margin) < 0.1).length ?? 0;
    return [
      {
        step: '1',
        title: 'Radius rule',
        metric: `${topology.radius_mode} · ${topology.radius.toFixed(4)} A`,
        detail: 'Atom-pair distances are compared with the hashkey radius; this decides which pairs become graph edges.'
      },
      {
        step: '2',
        title: 'Colored graph',
        metric: `${topology.summary.node_count} nodes · ${topology.summary.edge_count} edges`,
        detail: `${partitionCount} species/color partitions constrain canonicalization so chemically different atoms are not silently exchanged.`
      },
      {
        step: '3',
        title: 'Dreadnaut input',
        metric: `${topology.graph_text?.split('\n').filter(Boolean).length ?? 0} lines`,
        detail: 'The graph program is the exact text handed to dreadnaut for canonical labeling.'
      },
      {
        step: '4',
        title: 'Canonical hashkey',
        metric: shortHashkey(topology.canonical_hashkey),
        detail: topology.hashkey_error || `${nearCritical} near-cutoff pairs are close enough to explain possible hashkey flips under perturbation.`
      }
    ];
  }

  function focusTopologyNode(node) {
    topologyHoveredNode = node;
  }

  function clearTopologyNode(nodeIndex) {
    if (topologyPinnedNodeIndex === nodeIndex) return;
    if (topologyHoveredNode?.index === nodeIndex) {
      topologyHoveredNode = null;
    }
  }

  function toggleTopologyPin(nodeIndex) {
    topologyPinnedNodeIndex = topologyPinnedNodeIndex === nodeIndex ? null : nodeIndex;
  }

  function handleTopologyNodeKeydown(event, nodeIndex) {
    if (event.key === 'Enter' || event.key === ' ') {
      event.preventDefault();
      toggleTopologyPin(nodeIndex);
    }
  }

  async function invokeJson(command, args) {
    const raw = args === undefined ? await invoke(command) : await invoke(command, args);
    if (typeof raw !== 'string') return raw;
    try {
      return JSON.parse(raw);
    } catch (err) {
      const preview = raw.slice(0, 320).replace(/\s+/g, ' ');
      throw new Error(`${command} returned invalid JSON: ${err.message}. Payload starts: ${preview}`);
    }
  }

  async function refreshTopologyLab(runName = selectedRun?.run_name, structurePath = topologyStructurePath) {
    if (!runName && !structurePath) return;
    topologyLoading = true;
    topologyError = '';
    topologyHoveredNode = null;
    topologyPinnedNodeIndex = null;

    if (!isTauri()) {
      topologyLab = buildFallbackTopology(selectedRun);
      topologyLoading = false;
      return;
    }

    try {
      topologyLab = await invokeJson('load_topology_lab_json', {
        req: {
          run_name: runName || null,
          structure_path: structurePath || null
        }
      });
    } catch (err) {
      topologyError = `${err}`;
      topologyLab = null;
    } finally {
      topologyLoading = false;
    }
  }

  async function refreshSelectedRunDetail(runName = selectedRunName) {
    if (!runName) return;
    const requestKey = `${activeLane}:${runName}`;
    if (selectedRunDetailRequestedKey === requestKey) return;
    selectedRunDetailRequestedKey = requestKey;
    selectedRunDetailLoading = true;
    selectedRunDetailError = '';

    if (!isTauri()) {
      selectedRunDetail = catalog.runs.find((run) => run.run_name === runName) ?? null;
      selectedRunDetailLoading = false;
      return;
    }

    try {
      selectedRunDetail = await invokeJson('load_workspace_run_json', { req: { run_name: runName } });
    } catch (err) {
      selectedRunDetail = catalog.runs.find((run) => run.run_name === runName) ?? null;
      selectedRunDetailError = `${err}`;
    } finally {
      selectedRunDetailLoading = false;
    }
  }

  async function refreshCandidateStructurePreview() {
    const option = structureOptions.find((entry) => entry.key === selectedStructureKey) ?? structureOptions[0];
    const requestKey = `${selectedRun?.run_name || 'none'}:${option?.key || 'none'}`;
    if (candidateStructureRequestedKey === requestKey) return;
    candidateStructureRequestedKey = requestKey;
    if (!option || option.key === 'best' || !option.path) {
      candidateStructurePreview = null;
      candidateStructureError = '';
      candidateStructureLoading = false;
      return;
    }

    if (!isTauri()) {
      candidateStructurePreview = null;
      candidateStructureError = 'Alternate candidate loading requires the Tauri desktop runtime.';
      candidateStructureLoading = false;
      return;
    }

    candidateStructureLoading = true;
    candidateStructureError = '';
    try {
      candidateStructurePreview = await invokeJson('load_structure_preview_json', {
        req: { path: option.path }
      });
    } catch (err) {
      candidateStructurePreview = null;
      candidateStructureError = `${err}`;
    } finally {
      candidateStructureLoading = false;
    }
  }

  async function refreshCatalogOnly() {
    if (!isTauri()) return;

    try {
      catalog = await invokeJson('load_workspace_catalog_json');
      liveCatalogRefreshedAt = Date.now();
      selectedRunDetailRequestedKey = '';

      if (!catalog.runs.some((run) => run.run_name === selectedRunName)) {
        selectedRunName = preferredDefaultRunName(catalog.runs);
      }
      if (!catalog.runs.some((run) => run.run_name === igaPreferredRunName)) {
        igaPreferredRunName = preferredDefaultRunName(catalog.runs);
      }
    } catch (err) {
      error = `${err}`;
    }
  }

  async function refreshTauriSlices() {
    const [nextOverview, nextFoundation, nextFlowLab, nextAutoemulateLab, nextDatabase, nextCatalog] = await Promise.all([
      invoke('load_app_overview'),
      invoke('load_data_foundation'),
      invoke('load_data_flow_lab'),
      invokeJson('load_autoemulate_lab_json'),
      invoke('connect_data_store'),
      invokeJson('load_workspace_catalog_json')
    ]);

    overview = nextOverview;
    dataFoundation = nextFoundation;
    dataFlowLab = nextFlowLab;
    autoemulateLab = nextAutoemulateLab;
    database = nextDatabase;
    catalog = nextCatalog;
    liveCatalogRefreshedAt = Date.now();
    selectedRunDetailRequestedKey = '';
    if (!autoemulateLab.experiments.some((experiment) => autoemulateExperimentKey(experiment) === selectedAutoemulateExperimentKey)) {
      selectedAutoemulateExperimentKey =
        autoemulateLab.experiments.find((experiment) => experiment.feature_family === 'pair_distance_signature')?.artifact_dir ??
        autoemulateLab.experiments[0]?.artifact_dir ??
        '';
    }
  }

  async function refresh() {
    loading = true;
    error = '';

    if (!isTauri()) {
      backendMode = 'browser-preview';
      dataFlowLab = fallbackDataFlow;
      autoemulateLab = fallbackAutoemulateLab;
      loading = false;
      return;
    }

    try {
      backendMode = 'tauri';
      await refreshTauriSlices();

      if (!catalog.runs.some((run) => run.run_name === selectedRunName)) {
        selectedRunName = preferredDefaultRunName(catalog.runs);
      }
      if (!catalog.runs.some((run) => run.run_name === igaPreferredRunName)) {
        igaPreferredRunName = preferredDefaultRunName(catalog.runs);
      }
    } catch (err) {
      error = `${err}`;
      backendMode = 'fallback-after-error';
      overview = fallbackOverview;
      dataFoundation = fallbackDataFoundation;
      dataFlowLab = fallbackDataFlow;
      autoemulateLab = fallbackAutoemulateLab;
      database = fallbackDatabase;
      catalog = fallbackCatalog;
    } finally {
      loading = false;
    }
  }

  async function syncCatalogToStore() {
    if (!isTauri()) {
      error = 'Catalog sync requires the Tauri desktop runtime.';
      return;
    }
    syncing = true;
    try {
      lastSyncReport = await invokeJson('sync_workspace_catalog_to_store_json');
      await refresh();
    } catch (err) {
      error = `${err}`;
    } finally {
      syncing = false;
    }
  }

  async function runMaterialFlow() {
    const flow = selectedMaterialFlow();
    materialFlowError = '';
    if (!flow?.runnable) {
      materialFlowError = `${flow?.label || 'This flow'} is configured in its own app lane for now.`;
      return;
    }
    if (!isTauri()) {
      materialFlowError = 'Material flows require the Tauri desktop runtime.';
      return;
    }

    materialFlowLoading = true;
    try {
      if (selectedMaterialFlowId === 'surface_generation') {
        const result = await invokeJson('preview_surface_lab_json', {
          req: {
            cif_path: slabFlowPath,
            h,
            k,
            l,
            thickness_angstrom: thickness,
            vacuum_angstrom: vacuum,
            repeat_a: repeatA,
            repeat_b: repeatB,
            dedup_slab: dedupSlab
          }
        });
        slabFlowResult = result;
        materialFlowResult = buildSurfaceFlowResult(result, slabFlowPath);
      } else if (selectedMaterialFlowId === 'topology_identity') {
        const result = await invokeJson('load_topology_lab_json', {
          req: {
            run_name: selectedRunName || null,
            structure_path: topologyStructurePath || selectedStructurePath || null
          }
        });
        topologyLab = result;
        materialFlowResult = buildTopologyFlowResult(result, activeRun);
      } else {
        materialFlowResult = await invokeJson('preview_material_flow_json', {
          req: {
            flow_id: selectedMaterialFlowId,
            cif_path: materialFlowCifPath,
            engine: materialFlowEngine,
            tolerance: Number(materialFlowTolerance),
            normalization_target: materialFlowNormalizationTarget,
            perturbation_sigma: Number(perturbationSigma),
            perturbation_count: Number(perturbationCount),
            perturbation_seed: Number(perturbationSeed),
            perturbation_max_displacement: Number(perturbationMaxDisplacement),
            perturbation_min_distance: Number(perturbationMinDistance),
            perturbation_mode: perturbationMode
          }
        });
      }
      reusableFlowStructure = {
        label: materialFlowResult.output_kind || materialFlowResult.input_kind,
        source: materialFlowResult.source_path,
        flow_id: materialFlowResult.flow_id,
        preview: materialFlowResult.output_preview || materialFlowResult.input_preview
      };
    } catch (err) {
      materialFlowResult = null;
      materialFlowError = `${err}`;
    } finally {
      materialFlowLoading = false;
    }
  }

  function buildSurfaceFlowResult(result, sourcePath) {
    return {
      flow_id: 'surface_generation',
      title: 'Generate slab',
      source_path: sourcePath,
      engine: 'patina-surface',
      status: 'computed',
      input_kind: 'CIF framework',
      output_kind: 'surface slab',
      metrics: [
        { label: 'Miller', value: `(${result.diagnostics.miller.join(' ')})`, detail: null },
        { label: 'Parent atoms', value: `${result.diagnostics.parent_atom_count}`, detail: result.parent_label },
        { label: 'Slab atoms', value: `${result.diagnostics.slab_atom_count}`, detail: result.diagnostics.slab_label },
        { label: 'd_hkl', value: result.diagnostics.interplanar_spacing_angstrom?.toFixed?.(4) ?? '-', detail: 'angstrom' }
      ],
      operations: [
        { label: 'cut', kind: 'surface', detail: `offset ${result.diagnostics.chosen_cut_offset_angstrom?.toFixed?.(4) ?? '-'}` },
        { label: 'bond screen', kind: 'topology', detail: `safe cut ${result.diagnostics.topology_safe_cut ?? '-'}` },
        { label: 'reduction', kind: 'slab', detail: `broken bonds ${result.diagnostics.broken_bond_estimate ?? '-'}` }
      ],
      artifacts: [{ label: 'Slab preview', path: 'in-memory:patina-surface/slab_preview' }],
      input_preview: result.parent_preview,
      output_preview: result.slab_preview
    };
  }

  function buildTopologyFlowResult(result, run) {
    return {
      flow_id: 'topology_identity',
      title: 'Topology identity',
      source_path: result.source_path || run?.path || run?.run_name || selectedRunName,
      engine: 'patina-dreadnaut',
      status: 'computed',
      input_kind: 'workspace run structure',
      output_kind: 'canonical graph identity',
      metrics: [
        { label: 'Hashkey', value: shortHashkey(result.canonical_hashkey), detail: result.hashkey_error || null },
        { label: 'Nodes', value: `${result.summary.node_count}`, detail: `${result.summary.connected_components} components` },
        { label: 'Edges', value: `${result.summary.edge_count}`, detail: `radius ${result.radius.toFixed(4)} A` },
        { label: 'Near cutoffs', value: `${result.near_cutoff_pairs?.length ?? 0}`, detail: result.radius_mode }
      ],
      operations: (result.near_cutoff_pairs ?? []).slice(0, 10).map((pair, index) => ({
        label: `pair ${index + 1}`,
        kind: 'cutoff margin',
        detail: `${pair.left_species}${pair.left}-${pair.right_species}${pair.right}: ${pair.margin.toFixed(4)} A`
      })),
      artifacts: [{ label: 'dreadnaut graph', path: 'in-memory:patina-dreadnaut/graph_text' }],
      input_preview: result.source_preview || run?.structure_preview,
      output_preview: null
    };
  }

  function selectRun(runName) {
    selectedRunName = runName;
    igaPreferredRunName = runName;
    selectedRunDetailRequestedKey = '';
    selectedRunDetail = null;
    selectedRunDetailError = '';
    selectedStructureKey = 'best';
    candidateStructurePreview = null;
    candidateStructureError = '';
    candidateStructureLoading = false;
    candidateStructureRequestedKey = '';
  }

  function selectStructureOption(key) {
    selectedStructureKey = key;
    candidateStructurePreview = null;
    candidateStructureError = '';
    candidateStructureRequestedKey = '';
  }

  function viewCandidateStructure(candidate) {
    selectStructureOption(structureOptionKey(candidate));
  }

  function runFormula(run) {
    return run.best_structure?.formula || '-';
  }

  function mergeRunState(summaryRun, detailRun) {
    if (!summaryRun) return detailRun ?? null;
    if (!detailRun || detailRun.run_name !== summaryRun.run_name) return summaryRun;

    return {
      ...detailRun,
      ...summaryRun,
      artifacts: detailRun.artifacts ?? summaryRun.artifacts,
      best_evaluation_record: detailRun.best_evaluation_record ?? summaryRun.best_evaluation_record,
      best_structure_record: detailRun.best_structure_record ?? summaryRun.best_structure_record,
      ga_generations: detailRun.ga_generations?.length ? detailRun.ga_generations : summaryRun.ga_generations,
      top_candidates: detailRun.top_candidates?.length ? detailRun.top_candidates : summaryRun.top_candidates,
      ga_provenance_notes:
        detailRun.ga_provenance_notes?.length ? detailRun.ga_provenance_notes : summaryRun.ga_provenance_notes,
      structure_preview: summaryRun.structure_preview ?? detailRun.structure_preview,
      best_structure: summaryRun.best_structure ?? detailRun.best_structure,
      ga_live: summaryRun.ga_live ?? detailRun.ga_live,
      catalog_notes: summaryRun.catalog_notes ?? detailRun.catalog_notes
    };
  }

  $: selectedRun = catalog.runs.find((run) => run.run_name === selectedRunName) ?? catalog.runs[0] ?? null;
  $: activeRun = mergeRunState(selectedRun, selectedRunDetail);
  $: clusterRunCount = catalog.runs.filter((run) => run.best_structure?.dimensionality === '0D').length;
  $: selectedTopCandidates = topCandidatesForRun(activeRun);
  $: structureOptions = structureOptionsForRun(activeRun);
  $: selectedStructurePreview =
    selectedStructureKey === 'best' ? activeRun?.structure_preview ?? null : candidateStructurePreview;
  $: if (activeLane === 'surface') {
    activeLane = 'flows';
    selectedMaterialFlowId = 'surface_generation';
  }
  $: activeMaterialFlow = selectedMaterialFlow();
  $: if (activeMaterialFlow && !activeMaterialFlow.engines.includes(materialFlowEngine)) {
    materialFlowEngine = activeMaterialFlow.engines[0] ?? 'moyo';
  }
  $: selectedRunDbChain = activeRun
    ? [
        { label: 'Import source', detail: activeRun.artifacts.manifest_path || activeRun.path },
        { label: 'Canonical structure row', detail: `${activeRun.run_name} -> structures::best` },
        { label: 'Evaluation link', detail: activeRun.best_structure ? `best energy ${formatEnergy(activeRun.best_structure.energy)} eV` : 'pending evaluation row' },
        { label: 'Artifact anchors', detail: activeRun.artifacts.structure_exports || activeRun.artifacts.search_summary || 'manifest + declared artifacts' },
        { label: 'Next consumers', detail: 'Topology Lab, Generate slab flow, Sampling Lab, AutoEmulate, future peer sync' }
      ]
    : [];
  $: selectedStructureOption =
    structureOptions.find((option) => option.key === selectedStructureKey) ?? structureOptions[0] ?? null;
  $: selectedStructurePath = selectedStructureOption?.path || '';
  $: topologySourcePreview = topologyLab?.source_preview || selectedStructurePreview || activeRun?.structure_preview || null;
  $: if (activeLane === 'topology' && (selectedRun?.run_name || topologyStructurePath || selectedStructurePath)) {
    const resolvedTopologyPath = topologyStructurePath || selectedStructurePath || '';
    const nextKey = `${activeLane}:${selectedRun?.run_name || 'path'}:${resolvedTopologyPath}`;
    if (topologyRequestedKey !== nextKey) {
      topologyRequestedKey = nextKey;
      void refreshTopologyLab(selectedRun?.run_name, resolvedTopologyPath);
    }
  }
  $: topologyFocusedNodeIndex = topologyPinnedNodeIndex ?? topologyHoveredNode?.index ?? null;
  $: topologyFocusedNode = topologyLab?.graph?.nodes?.find(
    (node) => node.index === topologyFocusedNodeIndex
  ) ?? null;
  $: topologyProjection = buildTopologyProjection(
    topologySourcePreview,
    topologyLab?.graph,
    topologyFocusedNodeIndex,
    { width: 560, height: 440, padding: 34 }
  );
  $: topologyMatrix = buildTopologyMatrix(topologyLab?.graph, topologyFocusedNodeIndex);
  $: topologyHistogramRows = topologyLab?.summary?.degree_histogram_by_species
    ? Object.entries(topologyLab.summary.degree_histogram_by_species).flatMap(([species, histogram]) =>
        Object.entries(histogram).map(([degree, count]) => ({ species, degree, count }))
      )
    : [];
  $: topologyIdentityLayers = buildTopologyIdentityLayers(topologyLab);
  $: gaLive = activeRun?.ga_live ?? null;
  $: gaObservations = generationObservations(activeRun);
  $: latestGaObservation = gaObservations.at(-1) ?? null;
  $: if (!structureOptions.some((option) => option.key === selectedStructureKey)) {
    selectedStructureKey = 'best';
  }
  $: gaHealthCards = buildGaHealthCards(gaObservations, gaLive, selectedTopCandidates);
  $: gaWarningNotes = [
    ...(activeRun?.catalog_notes ?? []),
    ...(activeRun?.ga_provenance_notes ?? [])
  ].filter((note, index, notes) => note && notes.indexOf(note) === index);
  $: gaPopulationPlot = buildPopulationPlot(gaLive?.population);
  $: filteredPopulationCount =
    gaLive?.population?.filter((member) => isReasonableEnergy(member.energy)).length ?? 0;
  $: gaTrendSeries = buildGaTrendSeries(gaObservations);
  $: samplingExperiments = buildSamplingExperiments(activeRun);
  $: samplingTrendSeries = buildSamplingTrendSeries(samplingExperiments);
  $: samplingAcceptanceRows = buildSamplingAcceptanceRows(samplingExperiments);
  $: selectedAutoemulateExperiment = selectedAutoemulateExperimentFromLab(
    autoemulateLab,
    selectedAutoemulateExperimentKey
  );
  $: autoemulatePredictionSeries = buildAutoemulatePredictionSeries(selectedAutoemulateExperiment);
  $: autoemulateAcquisitionSeries = buildAutoemulateAcquisitionSeries(selectedAutoemulateExperiment);
  $: autoemulateFeatureSeries = buildAutoemulateExperimentSeries(
    autoemulateLab.experiments,
    'feature_count',
    'Feature count',
    '#0f5f70'
  );
  $: autoemulateSveltePlotRows = buildSveltePlotExperimentRows(autoemulateLab.experiments);
  $: autoemulateHeatmap = buildAutoemulateHeatmap(autoemulateLab.experiments);
  $: autoemulateDescriptorGroups = descriptorOptions(selectedAutoemulateExperiment);
  $: if (!autoemulateDescriptorGroups.includes(autoemulateDescriptorGroup)) {
    autoemulateDescriptorGroup = 'all';
  }
  $: selectedAutoemulateDescriptorIndices = selectedDescriptorIndices(
    selectedAutoemulateExperiment,
    autoemulateDescriptorGroup,
    autoemulateFeatureLimit
  );
  $: autoemulateProjection = buildDescriptorProjection(
    selectedAutoemulateExperiment,
    selectedAutoemulateDescriptorIndices
  );
  $: autoemulateSteeredQueue = buildSteeredQueue(
    selectedAutoemulateExperiment,
    autoemulateSteeringMode,
    autoemulateBiasedCandidateIds
  );
  $: autoemulateCandidateHeatmap = buildCandidateFeatureHeatmap(
    selectedAutoemulateExperiment,
    selectedAutoemulateDescriptorIndices,
    autoemulateSteeredQueue
  );
  $: if (activeLane === 'cluster' && selectedRun?.run_name) {
    void refreshSelectedRunDetail(selectedRun.run_name);
    if (selectedStructureKey !== 'best') {
      void refreshCandidateStructurePreview();
    }
  }
  $: igaLaunchCards = [
    { label: 'Seed run', value: igaPreferredRunName || 'pending' },
    { label: 'Engine', value: 'GULP' },
    { label: 'Population x generations', value: `${igaPopulation} x ${igaGenerations}` },
    { label: 'Temperature', value: `${igaTemperature} K` },
    {
      label: 'Mutation / diversity',
      value: `${Number(igaMutationWeight).toFixed(2)} / ${Number(igaDiversityWeight).toFixed(2)}`
    },
    { label: 'Execution posture', value: 'app mode local' }
  ];

  onMount(() => {
    refresh();

    const intervalId = setInterval(() => {
      if (!isTauri()) return;
      if (activeLane === 'cluster' || activeLane === 'topology') {
        void refreshCatalogOnly();
      }
    }, 4000);

    return () => clearInterval(intervalId);
  });
</script>

<svelte:head>
  <title>PATINA App</title>
</svelte:head>

<div class="shell">
  <header class="hero">
    <div>
      <p class="eyebrow">PATINA desktop workbench</p>
      <h1>{overview.app_name}</h1>
      <p class="lede">
        The program is now framed around three modes: `uLab mode` for HPC orchestration,
        `execution mode` for one concrete program-entry contract, and `app mode` for local Tauri
        execution with database-backed scientific state. The live GA runs pane sits inside app mode.
      </p>
    </div>
    <div class="hero-actions">
      <button on:click={refresh} disabled={loading}>{#if loading}Refreshing...{:else}Refresh{/if}</button>
      <button class="secondary" on:click={syncCatalogToStore} disabled={loading || syncing}>
        {#if syncing}Syncing DB...{:else}Sync DB{/if}
      </button>
      <span class="mode-chip">{backendMode}</span>
    </div>
  </header>

  {#if error}
    <div class="alert">{error}</div>
  {/if}

  <section class="mode-strip">
    {#each programModes as mode}
      <article class="mode-card">
        <p class="mini-label">{mode.posture}</p>
        <strong>{mode.name}</strong>
        <p>{mode.summary}</p>
      </article>
    {/each}
  </section>

  <div class="mode-banner">
    <strong>App mode workbench</strong>
    <span>
      These tabs are not the top-level program lanes. They are app-mode panes layered on top of the
      local runtime, the live workspace catalog, and the embedded SurrealDB graph.
    </span>
  </div>

  <nav class="lane-tabs" aria-label="app mode panes">
    <button class:active={activeLane === 'home'} on:click={() => (activeLane = 'home')}>Home</button>
    <button class:active={activeLane === 'data'} on:click={() => (activeLane = 'data')}>Data Graph</button>
    <button class:active={activeLane === 'cluster'} on:click={() => (activeLane = 'cluster')}>GA Runs</button>
    <button class:active={activeLane === 'sampling'} on:click={() => (activeLane = 'sampling')}>Sampling Lab</button>
    <button class:active={activeLane === 'autoemulate'} on:click={() => (activeLane = 'autoemulate')}>AutoEmulate</button>
    <button class:active={activeLane === 'flows'} on:click={() => (activeLane = 'flows')}>Flows</button>
    <button class:active={activeLane === 'topology'} on:click={() => (activeLane = 'topology')}>Topology Lab</button>
  </nav>

  {#if activeLane === 'home'}
    <section class="panel-grid">
      <article class="panel">
        <div class="panel-head">
          <h2>Database state</h2>
          <span>{database.connected ? 'connected' : 'pending'}</span>
        </div>
        <dl class="stats">
          <div><dt>Engine</dt><dd>{database.engine}</dd></div>
          <div><dt>Namespace</dt><dd>{database.namespace}</dd></div>
          <div><dt>Database</dt><dd>{database.database}</dd></div>
          <div><dt>Workspace root</dt><dd>{catalog.runs_root}</dd></div>
        </dl>
        <p class="callout">{dataFoundation.thesis}</p>
      </article>

      <article class="panel">
        <div class="panel-head">
          <h2>Current DB actions</h2>
          <span>explicit app-level controls</span>
        </div>
        <div class="action-grid">
          <button on:click={refresh} disabled={loading}>Reconnect + reload</button>
          <button on:click={syncCatalogToStore} disabled={loading || syncing}>Sync workspace catalog</button>
          <button on:click={() => (activeLane = 'data')}>Open data graph</button>
          <button on:click={() => (activeLane = 'cluster')}>Open GA runs lane</button>
          <button on:click={() => (activeLane = 'sampling')}>Open sampling lab</button>
          <button on:click={() => (activeLane = 'autoemulate')}>Open AutoEmulate tab</button>
          <button on:click={() => (activeLane = 'flows')}>Open flows</button>
          <button on:click={() => (activeLane = 'topology')}>Open topology lab</button>
        </div>
        {#if lastSyncReport}
          <pre>{JSON.stringify(lastSyncReport, null, 2)}</pre>
        {/if}
      </article>

      <article class="panel panel-wide">
        <div class="panel-head">
          <h2>Authoritative tables</h2>
          <span>what the user should eventually query directly</span>
        </div>
        <div class="card-grid">
          {#each dataFoundation.table_blueprint as table}
            <article class="mini-card">
              <strong>{table.table}</strong>
              <p>{table.role}</p>
            </article>
          {/each}
        </div>
      </article>

      <article class="panel panel-wide">
        <div class="panel-head">
          <h2>Workspace snapshot</h2>
          <span>{catalog.discovered} discovered runs</span>
        </div>
        <div class="card-grid">
          <article class="mini-card">
            <strong>{clusterRunCount}</strong>
            <p>cluster-oriented runs detected from current catalog evidence</p>
          </article>
          <article class="mini-card">
            <strong>{catalog.runs.filter((run) => run.has_checkpoint).length}</strong>
            <p>runs with checkpoint continuity ready for deeper app monitoring</p>
          </article>
          <article class="mini-card">
            <strong>{catalog.runs.reduce((sum, run) => sum + run.artifacts.declared_artifacts, 0)}</strong>
            <p>declared artifacts already suitable for DB anchoring and retrieval</p>
          </article>
          <article class="mini-card">
            <strong>{overview.campaign_checkpoint}</strong>
            <p>active campaign checkpoint driving this app lane</p>
          </article>
        </div>
      </article>

      <article class="panel panel-wide">
        <div class="panel-head">
          <h2>What The DB Actually Does</h2>
          <span>current import and reuse posture</span>
        </div>
        <div class="card-grid">
          {#each dataFlowLab.import_pipeline.slice(0, 2) as step}
            <article class="mini-card">
              <strong>{step.stage}</strong>
              <p>{step.detail}</p>
            </article>
          {/each}
          {#each dataFlowLab.export_pipeline.slice(0, 2) as step}
            <article class="mini-card">
              <strong>{step.stage}</strong>
              <p>{step.detail}</p>
            </article>
          {/each}
        </div>
      </article>
    </section>
  {/if}

  {#if activeLane === 'data'}
    <section class="panel-grid">
      <article class="panel panel-wide">
        <div class="panel-head">
          <h2>Data graph</h2>
          <span>why SurrealDB exists in this app</span>
        </div>
        <p class="callout">
          The database is not a mirror of the filesystem. It is the portable scientific graph that
          tells the app which structures, runs, evaluations, artifacts, and provenance events belong
          together so the next workflow can be launched from durable identities instead of ad hoc paths.
        </p>
      </article>

      <article class="panel panel-wide">
        <div class="panel-head">
          <h2>Live table counts</h2>
          <span>current authority surface</span>
        </div>
        <div class="card-grid">
          {#each dataFlowLab.table_counts as table}
            <article class="mini-card">
              <strong>{table.table}</strong>
              <p>{table.count} rows</p>
              <p>{table.role}</p>
            </article>
          {/each}
        </div>
      </article>

      <article class="panel">
        <div class="panel-head">
          <h2>Import into DB</h2>
          <span>current working path</span>
        </div>
        <div class="flow-rail">
          {#each dataFlowLab.import_pipeline as step}
            <article class="flow-step">
              <strong>{step.stage}</strong>
              <p>{step.detail}</p>
            </article>
          {/each}
        </div>
      </article>

      <article class="panel">
        <div class="panel-head">
          <h2>Export out of DB</h2>
          <span>what the app can chain next</span>
        </div>
        <div class="flow-rail">
          {#each dataFlowLab.export_pipeline as step}
            <article class="flow-step">
              <strong>{step.stage}</strong>
              <p>{step.detail}</p>
            </article>
          {/each}
        </div>
      </article>

      <article class="panel">
        <div class="panel-head">
          <h2>Selected run chain</h2>
          <span>{selectedRun?.run_name || 'no selected run'}</span>
        </div>
        <div class="flow-rail">
          {#each selectedRunDbChain as step}
            <article class="flow-step compact-step">
              <strong>{step.label}</strong>
              <p>{step.detail}</p>
            </article>
          {/each}
        </div>
      </article>

      <article class="panel">
        <div class="panel-head">
          <h2>Current capabilities</h2>
          <span>already true in code</span>
        </div>
        <ul class="vision-list">
          {#each dataFlowLab.current_capabilities as item}
            <li>{item}</li>
          {/each}
        </ul>
      </article>

      <article class="panel panel-wide">
        <div class="panel-head">
          <h2>Workflow chains</h2>
          <span>why this is scientifically coherent</span>
        </div>
        <div class="card-grid">
          {#each dataFlowLab.workflow_chains as chain}
            <article class="mini-card">
              <strong>{chain.name}</strong>
              <ul class="vision-list compact-list">
                {#each chain.steps as step}
                  <li>{step}</li>
                {/each}
              </ul>
            </article>
          {/each}
        </div>
      </article>

      <article class="panel panel-wide">
        <div class="panel-head">
          <h2>Current gaps</h2>
          <span>what still needs native app work</span>
        </div>
        <ul class="vision-list">
          {#each dataFlowLab.current_gaps as item}
            <li>{item}</li>
          {/each}
        </ul>
      </article>
    </section>
  {/if}

  {#if activeLane === 'cluster'}
    <section class="panel-grid">
      <article class="panel panel-wide">
        <div class="panel-head">
          <h2>GA runs and interactive studio</h2>
          <span>app mode live monitor for generation-driven searches</span>
        </div>
        <div class="mode-inline-grid">
          <article class="mini-card">
            <strong>Current top-level lane</strong>
            <p>App mode is active here: the Tauri shell is reading local GA state and binding the important scientific records into SurrealDB.</p>
          </article>
          <article class="mini-card">
            <strong>Execution contract</strong>
            <p>The selected run is treated as one execution-mode entry with a fixed `GULP`-first engine policy and reproducible manifest-backed inputs.</p>
          </article>
          <article class="mini-card">
            <strong>Live refresh</strong>
            <p>While this pane is open, the catalog polls every 4 seconds so new generations and spawned population members redraw automatically.</p>
          </article>
        </div>
      </article>

      <article class="panel panel-wide">
        <div class="panel-head">
          <h2>GA run catalog</h2>
          <span>{catalog.runs.length} runs · last poll {formatTimestamp(liveCatalogRefreshedAt)}</span>
        </div>
        <div class="table-wrap">
          <table>
            <thead>
              <tr>
                <th>Formula</th>
                <th>Run</th>
                <th>Engine</th>
                <th>Status</th>
                <th>Generation</th>
                <th>Valid</th>
                <th>Top</th>
                <th>Energy (eV)</th>
              </tr>
            </thead>
            <tbody>
              {#each catalog.runs as run}
                <tr class:selected={selectedRun?.run_name === run.run_name} on:click={() => selectRun(run.run_name)}>
                  <td class="formula-markup">
                    {@html formatFormulaMarkup(run.best_structure?.formula)}
                  </td>
                  <td>{run.run_name}</td>
                  <td>{engineLabel(run)}</td>
                  <td><span class={`status-pill ${statusClass(run.catalog_status)}`}>{run.catalog_status}</span></td>
                  <td>{latestObservationForRun(run)?.generation ?? run.ga_live?.current_generation ?? '-'}</td>
                  <td>{formatPercent(latestObservationForRun(run)?.valid_population_size ?? run.ga_live?.population_size ?? 0, latestObservationForRun(run)?.population_size ?? run.ga_live?.population_size ?? 0)}</td>
                  <td>{topCandidatesForRun(run).length}</td>
                  <td>{formatEnergy(run.best_structure?.energy)}</td>
                </tr>
              {/each}
            </tbody>
          </table>
        </div>
      </article>

      <article class="panel">
        <div class="panel-head">
          <h2>Selected GA run</h2>
          <span>{activeRun?.run_name}</span>
        </div>
        <div class="viewer-toolbar">
          <label>
            View structure
            <select bind:value={selectedStructureKey} on:change={(event) => selectStructureOption(event.currentTarget.value)}>
              {#each structureOptions as option}
                <option value={option.key}>{option.label}</option>
              {/each}
            </select>
          </label>
          {#if candidateStructureLoading}
            <span class="viewer-status">loading</span>
          {:else if candidateStructureError}
            <span class="viewer-status viewer-status-error">load failed</span>
          {:else}
            <span class="viewer-status">
              {#if selectedStructureKey === 'best'}
                current best
              {:else}
                ranked candidate
              {/if}
            </span>
          {/if}
        </div>
        {#if selectedStructurePreview}
          <LazyMatterVizStructure
            structure={selectedStructurePreview}
            title={selectedStructureKey === 'best' ? 'Current best structure' : 'Ranked candidate'}
            context="GA run"
            height={390}
          />
        {/if}
        {#if candidateStructureError}
          <p class="error-note viewer-error">{candidateStructureError}</p>
        {/if}
        {#if selectedRunDetailLoading}
          <p class="ga-note">Loading run detail...</p>
        {:else if selectedRunDetailError}
          <p class="error-note viewer-error">{selectedRunDetailError}</p>
        {/if}
        <dl class="stats">
          <div><dt>Formula</dt><dd class="formula-markup">{@html formatFormulaMarkup(runFormula(activeRun))}</dd></div>
          <div><dt>Workflow</dt><dd>{activeRun?.workflow_owner || '-'}</dd></div>
          <div><dt>Engine</dt><dd>{engineLabel(activeRun)}</dd></div>
          <div><dt>Program mode</dt><dd>app mode local execution</dd></div>
          <div><dt>Parallel</dt><dd>{activeRun?.run_spec.parallel_contract || '-'}</dd></div>
          <div><dt>Generation</dt><dd>{gaLive?.current_generation ?? '-'}</dd></div>
          <div><dt>Checkpoint</dt><dd>{activeRun?.has_checkpoint ? 'yes' : 'no'}</dd></div>
          <div><dt>Live state</dt><dd>{formatTimestamp(gaLive?.updated_at_unix_ms)}</dd></div>
        </dl>
      </article>

      <article class="panel">
        <div class="panel-head">
          <h2>Live GA summary</h2>
          <span>{engineLabel(activeRun)}</span>
        </div>
        <div class="card-grid ga-health-grid">
          {#each gaHealthCards as item}
            <article class="mini-card metric-card">
              <span>{item.label}</span>
              <strong>{item.value}</strong>
              <p>{item.detail}</p>
            </article>
          {/each}
        </div>
        <dl class="stats stats-compact">
          <div><dt>Phase</dt><dd>{latestGaObservation?.phase || '-'}</dd></div>
          <div><dt>Failures</dt><dd>{latestGaObservation?.failure_count ?? 0}</dd></div>
          <div><dt>Failure kind</dt><dd>{failureKindSummary(latestGaObservation)}</dd></div>
          <div><dt>Live state</dt><dd>{formatTimestamp(gaLive?.updated_at_unix_ms)}</dd></div>
        </dl>
        {#if gaWarningNotes.length}
          <div class="provenance-notes">
            {#each gaWarningNotes.slice(0, 4) as note}
              <p>{note}</p>
            {/each}
          </div>
        {/if}
      </article>

      <article class="panel panel-wide">
        <div class="panel-head">
          <h2>Top 5 identified candidates</h2>
          <span>{selectedTopCandidates.length} candidates from ranked unique output or live population</span>
        </div>
        {#if selectedTopCandidates.length}
          <div class="table-wrap compact-table">
            <table>
              <thead>
                <tr>
                  <th>Unique</th>
                  <th>Population</th>
                  <th>Label</th>
                  <th>Origin</th>
                  <th>Energy (eV)</th>
                  <th>Occur.</th>
                  <th>Converged</th>
                  <th>Hashkey</th>
                  <th>View</th>
                </tr>
              </thead>
              <tbody>
                {#each selectedTopCandidates as candidate}
                  <tr>
                    <td>{candidate.unique_rank}</td>
                    <td>{candidate.population_rank !== null && candidate.population_rank !== undefined ? candidate.population_rank + 1 : '-'}</td>
                    <td>{candidate.label}</td>
                    <td>
                      <span class="origin-chip" style={`background:${originColor(candidate.origin)}22;color:${originColor(candidate.origin)};border-color:${originColor(candidate.origin)}55;`}>
                        {candidate.origin}
                      </span>
                    </td>
                    <td>{formatEnergy(candidate.energy)}</td>
                    <td>{candidate.occurrences}</td>
                    <td>{candidate.converged ? 'yes' : 'no'}</td>
                    <td class="mono-cell">{shortHashkey(candidate.canonical_hashkey)}</td>
                    <td>
                      {#if candidate.structure_path}
                        <button
                          type="button"
                          class="table-button"
                          on:click|stopPropagation={() => viewCandidateStructure(candidate)}
                        >
                          View
                        </button>
                      {:else}
                        <span class="path-cell">no file</span>
                      {/if}
                    </td>
                  </tr>
                {/each}
              </tbody>
            </table>
          </div>
        {:else}
          <div class="placeholder compact-placeholder">No ranked candidate export is available for this run yet.</div>
        {/if}
      </article>

      <article class="panel">
        <div class="panel-head">
          <h2>Interactive GA launch brief</h2>
          <span>same lane, compact operator controls</span>
        </div>
        <div class="form-grid compact-form">
          <label>
            Seed run
            <select bind:value={igaPreferredRunName}>
              {#each catalog.runs as run}
                <option value={run.run_name}>{run.run_name}</option>
              {/each}
            </select>
          </label>
          <label>
            Population {igaPopulation}
            <input type="range" min="4" max="48" step="1" bind:value={igaPopulation} />
          </label>
          <label>
            Generations {igaGenerations}
            <input type="range" min="2" max="40" step="1" bind:value={igaGenerations} />
          </label>
          <label>
            Temperature {igaTemperature} K
            <input type="range" min="100" max="1200" step="10" bind:value={igaTemperature} />
          </label>
          <label>
            Mutation {Number(igaMutationWeight).toFixed(2)}
            <input type="range" min="0" max="1" step="0.05" bind:value={igaMutationWeight} />
          </label>
          <label>
            Diversity {Number(igaDiversityWeight).toFixed(2)}
            <input type="range" min="0" max="1" step="0.05" bind:value={igaDiversityWeight} />
          </label>
        </div>
        <p class="callout ga-note">
          Interactive GA control belongs in the GA runs lane itself. For now this stays compact and
          expresses one local `GULP` execution brief rather than a separate studio lane.
        </p>
        <div class="card-grid dense-card-grid">
          {#each igaLaunchCards as item}
            <article class="mini-card">
              <strong>{item.label}</strong>
              <p>{item.value}</p>
            </article>
          {/each}
        </div>
      </article>

      <article class="panel panel-wide">
        <div class="panel-head">
          <h2>Generation propagation plot</h2>
          <span>best, mean, and worst energy update as generations spawn</span>
        </div>
        <div class="graph graph-tall">
          {#if gaTrendSeries.length}
            <Plot height={340} marginLeft={72} marginBottom={54} x={{ label: 'Generation', tickFormat: 'd' }} y={{ label: 'Energy (eV)', grid: true }}>
              <GridY />
              {#each gaTrendSeries as series}
                <Line data={seriesRows(series)} x="x" y="y" stroke={seriesStroke(series)} strokeWidth={3} strokeDasharray={seriesDash(series)} />
                <Dot data={seriesRows(series)} x="x" y="y" fill={seriesStroke(series)} stroke="#ffffff" r={5} />
              {/each}
              <AxisX />
              <AxisY />
            </Plot>
            <div class="graph-caption">
              <span>Generation index</span>
              <span>Best, mean, and worst energies are read from generation summary JSON files under `raw/`.</span>
            </div>
          {:else}
            <div class="placeholder">Generation history will appear once a GA state file is available.</div>
          {/if}
        </div>
      </article>

      <article class="panel panel-wide">
        <div class="panel-head">
          <h2>Current population energy plot</h2>
          <span>sorted live population snapshot for the selected generation</span>
        </div>
        <div class="graph">
          {#if gaPopulationPlot}
            <svg viewBox={`0 0 ${gaPopulationPlot.width} ${gaPopulationPlot.height}`} class="energy-svg" aria-label="GA population energy plot">
              {#each gaPopulationPlot.ticks as tick}
                <g>
                  <line
                    x1={gaPopulationChart.padLeft}
                    y1={tick.y}
                    x2={gaPopulationPlot.width - gaPopulationChart.padRight}
                    y2={tick.y}
                    class="energy-gridline"
                  />
                  <text x={gaPopulationChart.padLeft - 10} y={tick.y + 4} text-anchor="end" class="energy-axis-label">
                    {tick.value.toFixed(2)}
                  </text>
                </g>
              {/each}
              <line
                x1={gaPopulationChart.padLeft}
                y1={gaPopulationPlot.axisY}
                x2={gaPopulationPlot.width - gaPopulationChart.padRight}
                y2={gaPopulationPlot.axisY}
                class="energy-axis"
              />
              {#each gaPopulationPlot.points as point}
                <g>
                  <line x1={point.x} y1={point.y} x2={point.x} y2={gaPopulationPlot.axisY} class="population-stem" />
                  <circle cx={point.x} cy={point.y} r={4 + Math.min(point.occurrences, 4)} fill={point.color} class="population-point" />
                  <text x={point.x} y={gaPopulationPlot.axisY + 22} text-anchor="middle" class="energy-run-label">
                    {point.rank}
                  </text>
                </g>
              {/each}
            </svg>
            <div class="graph-caption">
              <span>Population rank</span>
              <span>{filteredPopulationCount} reasonable-energy members plotted; irrational sentinels are excluded.</span>
            </div>
          {:else}
            <div class="placeholder">Population points will appear after a live GA state loads.</div>
          {/if}
        </div>
        {#if gaLive?.population?.length}
          <div class="population-list">
            {#each gaLive.population.slice(0, 8) as member}
              <div class="population-row">
                <strong>{member.label}</strong>
                <span class="origin-chip" style={`background:${originColor(member.origin)}22;color:${originColor(member.origin)};border-color:${originColor(member.origin)}55;`}>
                  {member.origin}
                </span>
                <span>{formatEnergy(member.energy)} eV</span>
                <span>{member.converged ? 'converged' : 'pending'}</span>
              </div>
            {/each}
          </div>
        {/if}
      </article>
    </section>
  {/if}

  {#if activeLane === 'sampling'}
    <section class="panel-grid">
      <article class="panel panel-wide">
        <div class="panel-head">
          <h2>Sampling lab</h2>
          <span>Rust BH, annealing, and energy-lid provenance</span>
        </div>
        <div class="card-grid dense-card-grid">
          {#each samplingFamilies as family}
            <article class="mini-card">
              <strong>{family.name}</strong>
              <p><b>Status:</b> {family.status}</p>
              <p><b>Trace:</b> {family.trace}</p>
              <p><b>Artifact:</b> {family.artifact}</p>
            </article>
          {/each}
        </div>
      </article>

      <article class="panel panel-wide">
        <div class="panel-head">
          <h2>Experiment trace plot</h2>
          <span>{activeRun?.run_name || 'selected run'} seeded comparison</span>
        </div>
        <div class="graph graph-tall">
          <Plot height={340} marginLeft={72} marginBottom={54} x={{ label: 'Step', tickFormat: 'd' }} y={{ label: 'Best energy (eV)', grid: true }}>
            <GridY />
            {#each samplingTrendSeries as series}
              <Line data={seriesRows(series)} x="x" y="y" stroke={seriesStroke(series)} strokeWidth={3} />
              <Dot data={seriesRows(series)} x="x" y="y" fill={seriesStroke(series)} stroke="#ffffff" r={5} />
            {/each}
            <AxisX />
            <AxisY />
          </Plot>
          <div class="graph-caption">
            <span>Trace schema</span>
            <span>BH writes `walker_trace.csv`; anneal and lid write `mc_trace.csv` under method-specific trace roots.</span>
          </div>
        </div>
      </article>

      <article class="panel">
        <div class="panel-head">
          <h2>Acceptance health</h2>
          <span>trace-derived review cards</span>
        </div>
        <div class="sampling-bars">
          {#each samplingAcceptanceRows as row}
            <div class="sampling-bar-row">
              <strong>{row.method}</strong>
              <div class="sampling-bar-track">
                <span style={`width:${Math.round(row.rate * 100)}%`}></span>
              </div>
              <p>{row.accepted}/{row.total} accepted</p>
            </div>
          {/each}
        </div>
      </article>

      <article class="panel panel-wide">
        <div class="panel-head">
          <h2>Trace provenance matrix</h2>
          <span>what each plotted line means</span>
        </div>
        <div class="table-wrap compact-table">
          <table>
            <thead>
              <tr>
                <th>Method</th>
                <th>Trace</th>
                <th>Summary</th>
                <th>Rust provenance</th>
              </tr>
            </thead>
            <tbody>
              {#each samplingExperiments as experiment}
                <tr>
                  <td>{experiment.method}</td>
                  <td class="mono-cell">{experiment.artifact}</td>
                  <td class="mono-cell">{experiment.summary}</td>
                  <td>{experiment.provenance}</td>
                </tr>
              {/each}
            </tbody>
          </table>
        </div>
      </article>

    </section>
  {/if}

  {#if activeLane === 'autoemulate'}
    <section class="panel-grid">
      <article class="panel panel-wide">
        <div class="panel-head">
          <h2>AutoEmulate experiments</h2>
          <span>{autoemulateLab.discovered} artifact-backed runs</span>
        </div>
        <div class="viewer-toolbar">
          <label>
            Experiment
            <select bind:value={selectedAutoemulateExperimentKey}>
              {#each autoemulateLab.experiments as experiment}
                <option value={autoemulateExperimentKey(experiment)}>
                  {autoemulateDisplayName(experiment)}
                </option>
              {/each}
            </select>
          </label>
          <span class="viewer-status">{selectedAutoemulateExperiment?.model_variant || 'no model'}</span>
        </div>
      </article>

      <article class="panel panel-wide">
        <div class="panel-head">
          <h2>Experiment set</h2>
          <span>{selectedAutoemulateExperiment?.ga_run_name || 'GA source'}</span>
        </div>
        <div class="card-grid dense-card-grid">
          {#each autoemulateLab.experiments as experiment}
            <button
              type="button"
              class="mini-card experiment-card"
              class:selectedExperiment={autoemulateExperimentKey(experiment) === autoemulateExperimentKey(selectedAutoemulateExperiment)}
              on:click={() => (selectedAutoemulateExperimentKey = autoemulateExperimentKey(experiment))}
            >
              <strong>{autoemulateDisplayName(experiment)}</strong>
              <p>{experiment.selected_model_name}</p>
              <span class="mini-stats">
                <span><i>Features</i><b>{experiment.feature_count}</b></span>
                <span><i>Training</i><b>{experiment.training_observation_count}</b></span>
                <span><i>Pending</i><b>{experiment.pending_candidate_count}</b></span>
              </span>
            </button>
          {/each}
        </div>
      </article>

      <article class="panel">
        <div class="panel-head">
          <h2>Selected model</h2>
          <span>{selectedAutoemulateExperiment?.feature_family || '-'}</span>
        </div>
        <dl class="stats">
          <div><dt>Campaign</dt><dd>{selectedAutoemulateExperiment?.campaign_id || '-'}</dd></div>
          <div><dt>Fidelity</dt><dd>{selectedAutoemulateExperiment?.fidelity || '-'}</dd></div>
          <div><dt>Incumbent</dt><dd>{formatEnergy(selectedAutoemulateExperiment?.incumbent_target)}</dd></div>
          <div><dt>Checkpoint</dt><dd>{selectedAutoemulateExperiment?.checkpoint_path ? 'persisted' : 'not saved'}</dd></div>
          <div><dt>Best ranked</dt><dd>{selectedAutoemulateExperiment?.best_ranked_candidate_id || '-'}</dd></div>
          <div><dt>Top acq.</dt><dd>{selectedAutoemulateExperiment?.top_acquisition_score?.toFixed?.(3) ?? '-'}</dd></div>
          <div><dt>Top uncertainty</dt><dd>{selectedAutoemulateExperiment?.top_uncertainty_score?.toFixed?.(3) ?? '-'}</dd></div>
        </dl>
      </article>

      <article class="panel">
        <div class="panel-head">
          <h2>Feature families</h2>
          <span>descriptor scale</span>
        </div>
        <div class="graph svelteplot-shell">
          <Plot
            height={260}
            marginLeft={56}
            marginBottom={44}
            x={{ label: 'Experiment', tickFormat: 'd' }}
            y={{ label: 'Feature count', grid: true }}
          >
            <GridY />
            <Line data={autoemulateSveltePlotRows} x="index" y="feature_count" stroke="#0f5f70" strokeWidth={3} />
            <Dot data={autoemulateSveltePlotRows} x="index" y="feature_count" fill="#c86b3c" r={6} />
            <Pointer data={autoemulateSveltePlotRows} x="index" y="feature_count" maxDistance={40}>
              {#snippet children({ data })}
                <Text
                  {data}
                  x="index"
                  y="feature_count"
                  text={(d) => `${d.feature_family}: ${d.feature_count}`}
                  dy={-10}
                  fontSize={11}
                  fontWeight="700"
                  fill="#112128"
                />
              {/snippet}
            </Pointer>
            <AxisX />
            <AxisY />
          </Plot>
        </div>
      </article>

      <article class="panel panel-wide">
        <div class="panel-head">
          <h2>Descriptor steering</h2>
          <span>{selectedAutoemulateDescriptorIndices.length} selected features</span>
        </div>
        <div class="steering-toolbar">
          <label>
            Descriptor family
            <select bind:value={autoemulateDescriptorGroup}>
              {#each autoemulateDescriptorGroups as group}
                <option value={group}>{group}</option>
              {/each}
            </select>
          </label>
          <label>
            Feature window {autoemulateFeatureLimit}
            <input type="range" min="2" max={Math.max(selectedAutoemulateExperiment?.feature_names?.length ?? 2, 2)} step="1" bind:value={autoemulateFeatureLimit} />
          </label>
          <label>
            Steering mode
            <select bind:value={autoemulateSteeringMode}>
              <option value="hybrid">hybrid</option>
              <option value="user">user steered</option>
              <option value="unsupervised">unsupervised</option>
            </select>
          </label>
          <button type="button" class="secondary" on:click={() => (autoemulateBiasedCandidateIds = [])}>
            Clear bias
          </button>
        </div>
      </article>

      <article class="panel panel-wide">
        <div class="panel-head">
          <h2>Candidate map</h2>
          <span>{autoemulateProjection?.featureCount || 0} descriptors · PCA</span>
        </div>
        <div class="projection-map svelteplot-shell">
          {#if autoemulateProjection}
            <Plot
              height={420}
              marginLeft={64}
              marginBottom={56}
              x={{ label: autoemulateProjection.xLabel, grid: true }}
              y={{ label: autoemulateProjection.yLabel, grid: true }}
            >
              <GridY />
              <Dot
                data={autoemulateProjection.points}
                x="pc1"
                y="pc2"
                r="radius"
                fill="color"
                stroke="stroke"
                strokeWidth="strokeWidth"
                fillOpacity={0.88}
                onclick={(_event, point) => toggleAutoemulateBias(point.candidate_id)}
                onmouseenter={(_event, point) => (autoemulateHoveredCandidateId = point.candidate_id)}
                onmouseleave={() => (autoemulateHoveredCandidateId = null)}
              />
              <Text
                data={autoemulateProjection.points}
                x="pc1"
                y="pc2"
                text={(point) => point.rank}
                fill="#ffffff"
                fontSize={10}
                fontWeight="800"
                textAnchor="middle"
                dy={3}
              />
              <Pointer data={autoemulateProjection.points} x="pc1" y="pc2" maxDistance={44}>
                {#snippet children({ data })}
                  <Text
                    {data}
                    x="pc1"
                    y="pc2"
                    text={(point) => `${point.label || point.candidate_id} · acq ${point.acquisition_score.toFixed(2)}`}
                    dy={-16}
                    fontSize={11}
                    fontWeight="700"
                    fill="#112128"
                  />
                {/snippet}
              </Pointer>
              <AxisX />
              <AxisY />
            </Plot>
            {#if autoemulateHoveredCandidateId}
              <div class="map-tooltip">
                {autoemulateHoveredCandidateId}
              </div>
            {/if}
          {:else}
            <div class="placeholder">No candidate feature vectors are available for this experiment.</div>
          {/if}
        </div>
      </article>

      <article class="panel panel-wide">
        <div class="panel-head">
          <h2>Descriptor heatmap</h2>
          <span>top steered candidates by selected feature window</span>
        </div>
        <div class="feature-heatmap svelteplot-shell">
          {#if autoemulateCandidateHeatmap.cells.length}
            <Plot
              height={Math.max(300, autoemulateCandidateHeatmap.rows.length * 38 + 120)}
              marginLeft={190}
              marginBottom={112}
              x={{ label: 'Descriptor', tickRotate: -45 }}
              y={{ label: 'Candidate' }}
            >
              <Cell
                data={autoemulateCandidateHeatmap.cells}
                x="featureLabel"
                y="candidate"
                fill="color"
                stroke="#ffffff"
                strokeWidth={1.5}
                onclick={(_event, cell) => toggleAutoemulateBias(cell.candidate_id)}
                onmouseenter={(_event, cell) => (autoemulateHoveredCandidateId = cell.candidate_id)}
                onmouseleave={() => (autoemulateHoveredCandidateId = null)}
              />
              <Text
                data={autoemulateCandidateHeatmap.cells}
                x="featureLabel"
                y="candidate"
                text={(cell) => (Number.isFinite(cell.raw) ? cell.raw.toFixed(Math.abs(cell.raw) > 99 ? 0 : 2) : '-')}
                fill={(cell) => (cell.scaled > 0.62 ? '#ffffff' : '#112128')}
                fontSize={10}
                fontWeight="700"
              />
              <AxisX />
              <AxisY />
            </Plot>
          {:else}
            <div class="placeholder">No descriptor matrix is available for this experiment.</div>
          {/if}
        </div>
      </article>

      <article class="panel panel-wide">
        <div class="panel-head">
          <h2>Next experiment queue</h2>
          <span>{autoemulateSteeringMode} mode</span>
        </div>
        <div class="table-wrap compact-table">
          <table>
            <thead>
              <tr>
                <th>Queue</th>
                <th>Candidate</th>
                <th>User bias</th>
                <th>Steering score</th>
                <th>Predicted eV</th>
                <th>Uncertainty</th>
                <th>Acquisition</th>
              </tr>
            </thead>
            <tbody>
              {#each autoemulateSteeredQueue.slice(0, 8) as row, index}
                <tr class:selected={row.user_bias > 0} on:click={() => toggleAutoemulateBias(row.candidate_id)}>
                  <td>{index + 1}</td>
                  <td>{row.label || row.candidate_id}</td>
                  <td>{row.user_bias ? 'yes' : 'no'}</td>
                  <td>{row.steering_score.toFixed(3)}</td>
                  <td>{formatEnergy(row.predicted_mean)}</td>
                  <td>{row.uncertainty_score.toFixed(3)}</td>
                  <td>{row.acquisition_score.toFixed(3)}</td>
                </tr>
              {/each}
            </tbody>
          </table>
        </div>
      </article>

      <article class="panel panel-wide">
        <div class="panel-head">
          <h2>Prediction by rank</h2>
          <span>{autoemulateDisplayName(selectedAutoemulateExperiment)}</span>
        </div>
        <div class="graph svelteplot-shell">
          <Plot height={320} marginLeft={72} marginBottom={54} x={{ label: 'Candidate rank', tickFormat: 'd' }} y={{ label: 'Predicted energy (eV)', grid: true }}>
            <GridY />
            {#each autoemulatePredictionSeries as series}
              <Line data={seriesRows(series)} x="x" y="y" stroke={seriesStroke(series)} strokeWidth={3} />
              <Dot data={seriesRows(series)} x="x" y="y" fill={seriesStroke(series)} stroke="#ffffff" r={5} />
            {/each}
            <AxisX />
            <AxisY />
          </Plot>
        </div>
      </article>

      <article class="panel panel-wide">
        <div class="panel-head">
          <h2>Acquisition and uncertainty</h2>
          <span>candidate ranking</span>
        </div>
        <div class="graph svelteplot-shell">
          <Plot height={320} marginLeft={72} marginBottom={54} x={{ label: 'Candidate rank', tickFormat: 'd' }} y={{ label: 'Score', grid: true }}>
            <GridY />
            {#each autoemulateAcquisitionSeries as series}
              <Line data={seriesRows(series)} x="x" y="y" stroke={seriesStroke(series)} strokeWidth={3} strokeDasharray={seriesDash(series)} />
              <Dot data={seriesRows(series)} x="x" y="y" fill={seriesStroke(series)} stroke="#ffffff" r={5} />
            {/each}
            <AxisX />
            <AxisY />
          </Plot>
        </div>
      </article>

      <article class="panel panel-wide">
        <div class="panel-head">
          <h2>Experiment heatmap</h2>
          <span>normalised across discovered runs</span>
        </div>
        <div class="heatmap">
          <div class="heatmap-row heatmap-head">
            <span>Feature family</span>
            {#each autoemulateHeatmap.columns as column}
              <span>{column.label}</span>
            {/each}
          </div>
          {#each autoemulateHeatmap.rows as row}
            <div class="heatmap-row">
              <strong>{row.label}</strong>
              {#each row.cells as cell, cellIndex}
                <span style={heatmapCellStyle(cell, autoemulateHeatmap.ranges[cellIndex])}>
                  {Number.isFinite(cell.raw) ? cell.raw.toFixed(Math.abs(cell.raw) > 99 ? 0 : 2) : '-'}
                </span>
              {/each}
            </div>
          {/each}
        </div>
      </article>

      <article class="panel panel-wide">
        <div class="panel-head">
          <h2>Candidate table</h2>
          <span>{selectedAutoemulateExperiment?.predictions?.length || 0} predictions</span>
        </div>
        <div class="table-wrap compact-table">
          <table>
            <thead>
              <tr>
                <th>Rank</th>
                <th>Candidate</th>
                <th>Label</th>
                <th>Predicted eV</th>
                <th>Variance</th>
                <th>Uncertainty</th>
                <th>Acquisition</th>
              </tr>
            </thead>
            <tbody>
              {#each selectedAutoemulateExperiment?.predictions ?? [] as row}
                <tr>
                  <td>{row.rank}</td>
                  <td>{row.candidate_id}</td>
                  <td>{row.label || '-'}</td>
                  <td>{formatEnergy(row.predicted_mean)}</td>
                  <td>{row.predicted_variance.toFixed(3)}</td>
                  <td>{row.uncertainty_score.toFixed(3)}</td>
                  <td>{row.acquisition_score.toFixed(3)}</td>
                </tr>
              {/each}
            </tbody>
          </table>
        </div>
      </article>
    </section>
  {/if}

  {#if activeLane === 'flows'}
    <section class="panel-grid">
      <article class="panel panel-wide">
        <div class="panel-head">
          <h2>Materials flows</h2>
          <span>input -> method -> result</span>
        </div>
        <div class="flow-catalog">
          {#each materialFlowCatalog as flow}
            <button
              type="button"
              class="flow-card"
              class:selected={flow.id === selectedMaterialFlowId}
              on:click={() => (selectedMaterialFlowId = flow.id)}
            >
              <span>{flow.family}</span>
              <strong>{flow.label}</strong>
              <p>{flow.input} -> {flow.output}</p>
              <small>{flow.engines.join(' / ')}</small>
            </button>
          {/each}
        </div>
      </article>

      <article class="panel panel-wide">
        <div class="panel-head">
          <h2>{activeMaterialFlow?.label}</h2>
          <span>{activeMaterialFlow?.family}</span>
        </div>
        <div class="flow-run-grid flow-run-grid-owned">
          {#if selectedMaterialFlowId === 'surface_generation'}
            <label>
              Parent CIF
              <input bind:value={slabFlowPath} />
            </label>
            <label>
              Miller h
              <input type="number" step="1" bind:value={h} />
            </label>
            <label>
              Miller k
              <input type="number" step="1" bind:value={k} />
            </label>
            <label>
              Miller l
              <input type="number" step="1" bind:value={l} />
            </label>
            <label>
              Thickness A
              <input type="number" min="1" step="0.5" bind:value={thickness} />
            </label>
            <label>
              Vacuum A
              <input type="number" min="1" step="0.5" bind:value={vacuum} />
            </label>
            <label>
              Repeat a
              <input type="number" min="1" step="1" bind:value={repeatA} />
            </label>
            <label>
              Repeat b
              <input type="number" min="1" step="1" bind:value={repeatB} />
            </label>
            <label class="checkbox-row">
              <input type="checkbox" bind:checked={dedupSlab} />
              Deduplicate slab
            </label>
          {:else if selectedMaterialFlowId === 'topology_identity'}
            <label>
              Workspace run
              <select bind:value={selectedRunName}>
                {#each catalog.runs as run}
                  <option value={run.run_name}>{run.run_name}</option>
                {/each}
              </select>
            </label>
            <label class="wide-label">
              Optional structure path
              <input bind:value={topologyStructurePath} placeholder={selectedStructurePath || 'leave empty to use run artifacts'} />
            </label>
            <button type="button" class="secondary" on:click={() => (topologyStructurePath = selectedStructurePath)}>
              Use selected candidate
            </button>
          {:else if selectedMaterialFlowId === 'global_search'}
            <label>
              Run feed
              <select bind:value={selectedRunName}>
                {#each catalog.runs as run}
                  <option value={run.run_name}>{run.run_name}</option>
                {/each}
              </select>
            </label>
            <label>
              Structure focus
              <select bind:value={selectedStructureKey}>
                {#each structureOptions as option}
                  <option value={option.key}>{option.label}</option>
                {/each}
              </select>
            </label>
          {:else if selectedMaterialFlowId === 'perturb_cluster'}
            <label class="wide-label">
              Cluster XYZ
              <input bind:value={materialFlowCifPath} placeholder="path/to/cluster.xyz" />
            </label>
            <label>
              Sigma
              <input type="number" min="0.001" step="0.005" bind:value={perturbationSigma} />
            </label>
            <label>
              Variants
              <input type="number" min="1" max="48" step="1" bind:value={perturbationCount} />
            </label>
            <label>
              Mode
              <select bind:value={perturbationMode}>
                <option value="s">isotropic</option>
                <option value="sp">s/p-axis</option>
              </select>
            </label>
            <label>
              Seed
              <input type="number" step="1" bind:value={perturbationSeed} />
            </label>
            <label>
              Max step A
              <input type="number" min="0.001" step="0.01" bind:value={perturbationMaxDisplacement} />
            </label>
            <label>
              Min distance A
              <input type="number" min="0" step="0.05" bind:value={perturbationMinDistance} />
            </label>
          {:else}
            <label class="wide-label">
              Structure input
              <input bind:value={materialFlowCifPath} />
            </label>
            <label>
              Engine
              <select bind:value={materialFlowEngine}>
                {#each activeMaterialFlow?.engines ?? [] as engine}
                  <option value={engine}>{engine}</option>
                {/each}
              </select>
            </label>
            <label>
              Tolerance
              <input type="number" min="0.0000001" step="0.000001" bind:value={materialFlowTolerance} />
            </label>
            {#if selectedMaterialFlowId === 'normalize_framework'}
              <label>
                Target
                <select bind:value={materialFlowNormalizationTarget}>
                  <option value="standardized">standardized</option>
                  <option value="primitive_standardized">primitive standardized</option>
                </select>
              </label>
            {/if}
          {/if}
          <button type="button" on:click={runMaterialFlow} disabled={materialFlowLoading || !activeMaterialFlow?.runnable}>
            {#if materialFlowLoading}Running...{:else if selectedMaterialFlowId === 'global_search'}Inspect flow{:else}Run flow{/if}
          </button>
        </div>
        {#if !activeMaterialFlow?.runnable}
          <p class="callout">This flow is represented here as a type contract; execution stays in its dedicated lane until the IO/result schema is unified.</p>
        {:else if selectedMaterialFlowId === 'structure_symmetry' && materialFlowEngine === 'syva'}
          <p class="callout">SYVA accepts XYZ point geometries and CIF-derived Cartesian coordinates. Moyo-backed periodic symmetry and normalization require periodic CIF-style lattice provenance.</p>
        {/if}
        {#if materialFlowError}
          <p class="error-note">{materialFlowError}</p>
        {/if}
      </article>

      {#if selectedMaterialFlowId === 'global_search'}
        <article class="panel panel-wide">
          <div class="panel-head">
            <h2>Search traces</h2>
            <span>{activeRun?.run_name || 'selected run'}</span>
          </div>
          <div class="graph graph-tall">
            <Plot height={300} marginLeft={72} marginBottom={54} x={{ label: 'Step', tickFormat: 'd' }} y={{ label: 'Best energy (eV)', grid: true }}>
              <GridY />
              {#each samplingTrendSeries as series}
                <Line data={seriesRows(series)} x="x" y="y" stroke={seriesStroke(series)} strokeWidth={3} />
                <Dot data={seriesRows(series)} x="x" y="y" fill={seriesStroke(series)} stroke="#ffffff" r={5} />
              {/each}
              <AxisX />
              <AxisY />
            </Plot>
          </div>
        </article>

        <article class="panel">
          <div class="panel-head">
            <h2>Search families</h2>
            <span>global search contract</span>
          </div>
          <div class="sampling-bars">
            {#each samplingAcceptanceRows as row}
              <div class="sampling-bar-row">
                <strong>{row.method}</strong>
                <div class="sampling-bar-track">
                  <span style={`width:${Math.round(row.rate * 100)}%`}></span>
                </div>
                <p>{row.accepted}/{row.total} accepted</p>
              </div>
            {/each}
          </div>
        </article>
      {/if}

      {#if selectedMaterialFlowId === 'topology_identity' && topologyLab}
        <article class="panel panel-wide">
          <div class="panel-head">
            <h2>Topology graph</h2>
            <span>{shortHashkey(topologyLab.canonical_hashkey)}</span>
          </div>
          <div class="topology-banner">
            <div class="hashkey-card">
              <p class="mini-label">Canonical hashkey</p>
              <strong>{shortHashkey(topologyLab.canonical_hashkey)}</strong>
            </div>
            <div class="hashkey-card">
              <p class="mini-label">Radius</p>
              <strong>{topologyLab.radius_mode}</strong>
              <p>{topologyLab.radius.toFixed(4)} A</p>
            </div>
            <div class="hashkey-card">
              <p class="mini-label">Graph</p>
              <strong>{topologyLab.summary.node_count} nodes · {topologyLab.summary.edge_count} edges</strong>
              <p>{topologyLab.summary.connected_components} components</p>
            </div>
          </div>
        </article>
        <article class="panel panel-wide">
          <div class="panel-head">
            <h2>Hashkey calculation</h2>
            <span>graph identity pipeline</span>
          </div>
          <div class="identity-layer-grid">
            {#each topologyIdentityLayers as layer}
              <article class="identity-layer-card">
                <span>{layer.step}</span>
                <strong>{layer.title}</strong>
                <em>{layer.metric}</em>
                <p>{layer.detail}</p>
              </article>
            {/each}
          </div>
        </article>
      {/if}

      {#if selectedMaterialFlowId === 'surface_generation'}
        <article class="panel">
          <div class="panel-head">
            <h2>Input framework</h2>
            <span>{slabFlowResult?.parent_label || 'awaiting slab run'}</span>
          </div>
          {#if slabFlowResult?.parent_preview}
            <LazyMatterVizStructure
              structure={slabFlowResult.parent_preview}
              title="Parent framework"
              context="Generate slab input"
              height={390}
            />
          {:else}
            <div class="placeholder">Run Generate slab to inspect the parent framework.</div>
          {/if}
        </article>

        <article class="panel">
          <div class="panel-head">
            <h2>Generated slab</h2>
            <span>{slabFlowResult?.diagnostics?.slab_label || 'awaiting slab run'}</span>
          </div>
          {#if slabFlowResult?.slab_preview}
            <LazyMatterVizStructure
              structure={slabFlowResult.slab_preview}
              title="Generated slab"
              context="Generate slab output"
              height={390}
              startInfoOpen={true}
            />
          {:else}
            <div class="placeholder">The slab preview will appear here.</div>
          {/if}
        </article>

        <article class="panel panel-wide">
          <div class="panel-head">
            <h2>Slab diagnostics</h2>
            <span>surface provenance</span>
          </div>
          <dl class="stats">
            <div><dt>Miller</dt><dd>{slabFlowResult?.diagnostics ? `(${slabFlowResult.diagnostics.miller.join(' ')})` : '-'}</dd></div>
            <div><dt>Parent atoms</dt><dd>{slabFlowResult?.diagnostics?.parent_atom_count ?? '-'}</dd></div>
            <div><dt>Slab atoms</dt><dd>{slabFlowResult?.diagnostics?.slab_atom_count ?? '-'}</dd></div>
            <div><dt>Safe cut</dt><dd>{slabFlowResult?.diagnostics?.topology_safe_cut ?? '-'}</dd></div>
            <div><dt>d_hkl</dt><dd>{slabFlowResult?.diagnostics?.interplanar_spacing_angstrom?.toFixed?.(4) ?? '-'}</dd></div>
            <div><dt>Broken bonds</dt><dd>{slabFlowResult?.diagnostics?.broken_bond_estimate ?? '-'}</dd></div>
          </dl>
          {#if slabFlowResult?.diagnostics?.warnings?.length}
            <ul class="warning-list">
              {#each slabFlowResult.diagnostics.warnings as warning}
                <li>{warning}</li>
              {/each}
            </ul>
          {/if}
        </article>
      {/if}

      <article class="panel">
        <div class="panel-head">
          <h2>Flow IO</h2>
          <span>{materialFlowResult?.status || 'awaiting run'}</span>
        </div>
        <div class="flow-io-grid">
          <article>
            <span>Input</span>
            <strong>{materialFlowResult?.input_kind || activeMaterialFlow?.input}</strong>
            <p>{materialFlowResult?.source_path || materialFlowCifPath}</p>
          </article>
          <article>
            <span>Output</span>
            <strong>{materialFlowResult?.output_kind || activeMaterialFlow?.output}</strong>
            <p>{materialFlowResult?.engine || materialFlowEngine}</p>
          </article>
        </div>
      </article>

      <article class="panel">
        <div class="panel-head">
          <h2>Reusable structure</h2>
          <span>{reusableFlowStructure ? 'available' : 'none'}</span>
        </div>
        {#if reusableFlowStructure}
          <div class="flow-node-card">
            <strong>{reusableFlowStructure.label}</strong>
            <p>{reusableFlowStructure.source}</p>
            <button type="button" class="secondary" on:click={() => {
              if (reusableFlowStructure.source) {
                slabFlowPath = reusableFlowStructure.source;
                materialFlowCifPath = reusableFlowStructure.source;
              }
            }}>
              Reuse source path
            </button>
          </div>
        {:else}
          <div class="placeholder compact-placeholder">Run a flow to create a reusable handoff.</div>
        {/if}
      </article>

      <article class="panel">
        <div class="panel-head">
          <h2>Metrics</h2>
          <span>{materialFlowResult?.metrics?.length || 0} values</span>
        </div>
        {#if materialFlowResult?.metrics?.length}
          <dl class="stats">
            {#each materialFlowResult.metrics as metric}
              <div>
                <dt>{metric.label}</dt>
                <dd>{metric.value}</dd>
                {#if metric.detail}<p>{metric.detail}</p>{/if}
              </div>
            {/each}
          </dl>
        {:else}
          <div class="placeholder compact-placeholder">Run a flow to populate results.</div>
        {/if}
      </article>

      {#if selectedMaterialFlowId !== 'surface_generation'}
        <article class="panel panel-wide">
          <div class="panel-head">
            <h2>Structure previews</h2>
            <span>input and output</span>
          </div>
          <div class="flow-preview-grid">
            <div class="viewer">
              {#if materialFlowResult?.input_preview}
                <LazyMatterVizStructure
                  structure={materialFlowResult.input_preview}
                  title={materialFlowResult.input_kind}
                  context="Flow input"
                  height={360}
                  compact={true}
                />
              {:else}
                <div class="placeholder">Input preview appears after a run.</div>
              {/if}
            </div>
            <div class="viewer">
              {#if materialFlowResult?.output_preview}
                <LazyMatterVizStructure
                  structure={materialFlowResult.output_preview}
                  title={materialFlowResult.output_kind}
                  context="Flow output"
                  height={360}
                  compact={true}
                />
              {:else}
                <div class="placeholder">Some flows return operators without a structure transform.</div>
              {/if}
            </div>
          </div>
        </article>
      {/if}

      <article class="panel panel-wide">
        <div class="panel-head">
          <h2>Operators and artifacts</h2>
          <span>{materialFlowResult?.engine || activeMaterialFlow?.engines?.join(' / ')}</span>
        </div>
        <div class="flow-result-grid">
          <div class="table-wrap compact-table">
            <table>
              <thead>
                <tr>
                  <th>Operator</th>
                  <th>Kind</th>
                  <th>Detail</th>
                </tr>
              </thead>
              <tbody>
                {#each materialFlowResult?.operations ?? [] as operation}
                  <tr>
                    <td>{operation.label}</td>
                    <td>{operation.kind}</td>
                    <td class="mono-cell">{operation.detail}</td>
                  </tr>
                {/each}
              </tbody>
            </table>
          </div>
          <div class="flow-file-list">
            {#each materialFlowResult?.artifacts ?? [] as artifact}
              <span>{artifact.label}: {artifact.path}</span>
            {/each}
            {#if !materialFlowResult?.artifacts?.length}
              <span>Artifacts will appear here when the selected flow emits a materialized payload.</span>
            {/if}
          </div>
        </div>
      </article>
    </section>
  {/if}

  {#if activeLane === 'topology'}
    <section class="panel-grid">
      <article class="panel panel-wide topology-hero">
        <div class="panel-head">
          <h2>Topology lab</h2>
          <span>{topologyLab?.source_path || selectedRun?.run_name || 'no selected run'}</span>
        </div>
        <div class="flow-run-grid topology-inline-controls">
          <label>
            Run
            <select bind:value={selectedRunName}>
              {#each catalog.runs as run}
                <option value={run.run_name}>{run.run_name}</option>
              {/each}
            </select>
          </label>
          <label class="wide-label">
            Structure path
            <input bind:value={topologyStructurePath} placeholder={selectedStructurePath || 'use first exported candidate'} />
          </label>
          <button type="button" on:click={() => refreshTopologyLab(selectedRunName, topologyStructurePath || selectedStructurePath)}>
            Compute topology
          </button>
        </div>
        {#if topologyLoading}
          <div class="placeholder">Computing dreadnaut graph and canonical identity...</div>
        {:else if topologyError}
          <div class="error-note">{topologyError}</div>
        {:else if topologyLab}
          <div class="topology-banner">
            <div class="hashkey-card">
              <p class="mini-label">Canonical hashkey</p>
              <strong>{shortHashkey(topologyLab.canonical_hashkey)}</strong>
              {#if topologyLab.hashkey_error}
                <p>{topologyLab.hashkey_error}</p>
              {/if}
            </div>
            <div class="hashkey-card">
              <p class="mini-label">Radius rule</p>
              <strong>{topologyLab.radius_mode}</strong>
              <p>radius {topologyLab.radius.toFixed(4)} A · const {topologyLab.radius_const}</p>
            </div>
            <div class="hashkey-card">
              <p class="mini-label">Graph summary</p>
              <strong>{topologyLab.summary.node_count} nodes · {topologyLab.summary.edge_count} edges</strong>
              <p>{topologyLab.summary.connected_components} connected components</p>
            </div>
          </div>
        {/if}
      </article>

      <article class="panel">
        <div class="panel-head">
          <h2>Structure context</h2>
          <span>selected cluster</span>
        </div>
        {#if topologySourcePreview}
          <LazyMatterVizStructure
            structure={topologySourcePreview}
            title="Topology source"
            context="Graph input"
            height={360}
            selectedSites={topologyFocusedNodeIndex !== null && topologyFocusedNodeIndex !== undefined ? [topologyFocusedNodeIndex] : []}
          />
        {:else}
          <div class="placeholder">Choose a run from the cluster pane first.</div>
        {/if}
      </article>

      <article class="panel">
        <div class="panel-head">
          <h2>Spatial projection</h2>
          <span>best-fit plane with local topology focus</span>
        </div>
        <div class="topology-graph-shell">
          {#if topologyProjection}
            <svg
              viewBox={`0 0 ${topologyProjection.width} ${topologyProjection.height}`}
              class="topology-svg"
              aria-label="Topology projection"
            >
              <defs>
                <radialGradient id="topologyGlow" cx="50%" cy="50%" r="60%">
                  <stop offset="0%" stop-color="#ffffff" stop-opacity="0.85" />
                  <stop offset="100%" stop-color="#ffffff" stop-opacity="0" />
                </radialGradient>
              </defs>
              {#each topologyProjection.focusedEdges as edge}
                <line
                  x1={edge.left.x}
                  y1={edge.left.y}
                  x2={edge.right.x}
                  y2={edge.right.y}
                  class="topology-edge topology-edge-focus"
                />
              {/each}
              {#each topologyProjection.nodes as node}
                <g
                  transform={`translate(${node.x} ${node.y})`}
                  role="button"
                  tabindex="0"
                  aria-label={`Focus topology node ${node.index}`}
                  class:node-active={topologyFocusedNodeIndex === node.index}
                  on:mouseenter={() => focusTopologyNode(node)}
                  on:mouseleave={() => clearTopologyNode(node.index)}
                  on:click={() => toggleTopologyPin(node.index)}
                  on:keydown={(event) => handleTopologyNodeKeydown(event, node.index)}
                >
                  <circle r={18 + Math.min(node.degree, 5)} fill="url(#topologyGlow)" />
                  <circle r={11 + Math.min(node.degree, 4)} fill={node.color} class="topology-node" />
                  <text y="4" text-anchor="middle" class="topology-label">{node.species}</text>
                </g>
              {/each}
            </svg>
            <div class="graph-caption topology-caption">
              <span>{topologyProjection.method}</span>
              <span>Hover or pin a node to reveal only its local neighborhood instead of every edge at once.</span>
            </div>
          {:else}
            <div class="placeholder">No topology preview is available yet.</div>
          {/if}
        </div>
        {#if topologyFocusedNode}
          <div class="hover-note">
            node {topologyFocusedNode.index} · {topologyFocusedNode.species} · degree {topologyFocusedNode.degree}
          </div>
        {/if}
      </article>

      <article class="panel">
        <div class="panel-head">
          <h2>Adjacency matrix</h2>
          <span>full graph without line clutter</span>
        </div>
        <div class="matrix-shell">
          {#if topologyMatrix}
            <svg
              viewBox={`0 0 ${topologyMatrix.width} ${topologyMatrix.height}`}
              class="matrix-svg"
              aria-label="Topology adjacency matrix"
            >
              {#each topologyMatrix.order as nodeIndex, orderIndex}
                <text
                  x={topologyMatrix.inset - 10}
                  y={topologyMatrix.inset + orderIndex * topologyMatrix.cellSize + topologyMatrix.cellSize * 0.68}
                  text-anchor="end"
                  class="matrix-label"
                >
                  {nodeIndex}
                </text>
                <text
                  x={topologyMatrix.inset + orderIndex * topologyMatrix.cellSize + topologyMatrix.cellSize * 0.5}
                  y={topologyMatrix.inset - 12}
                  text-anchor="middle"
                  class="matrix-label"
                >
                  {nodeIndex}
                </text>
              {/each}
              {#each topologyMatrix.cells as cell}
                <rect
                  x={cell.x}
                  y={cell.y}
                  width={topologyMatrix.cellSize - 1}
                  height={topologyMatrix.cellSize - 1}
                  class:matrix-cell-active={cell.active}
                  class:matrix-cell-diagonal={cell.diagonal}
                  class:matrix-cell-focus={cell.emphasized}
                  class="matrix-cell"
                />
              {/each}
            </svg>
          {:else}
            <div class="placeholder">Matrix view will appear after topology loading.</div>
          {/if}
        </div>
      </article>

      <article class="panel panel-wide">
        <div class="panel-head">
          <h2>Hashkey calculation</h2>
          <span>radius -> graph -> canonical label</span>
        </div>
        <div class="identity-layer-grid">
          {#each topologyIdentityLayers as layer}
            <article class="identity-layer-card">
              <span>{layer.step}</span>
              <strong>{layer.title}</strong>
              <em>{layer.metric}</em>
              <p>{layer.detail}</p>
            </article>
          {/each}
        </div>
      </article>

      <article class="panel">
        <div class="panel-head">
          <h2>Degree histogram</h2>
          <span>species-resolved graph signature</span>
        </div>
        <div class="card-grid compact-grid">
          {#each topologyHistogramRows as row}
            <article class="mini-card">
              <strong>{row.species}</strong>
              <p>degree {row.degree}</p>
              <p>{row.count} nodes</p>
            </article>
          {/each}
        </div>
      </article>

      <article class="panel">
        <div class="panel-head">
          <h2>Near-cutoff pairs</h2>
          <span>why the hashkey can change</span>
        </div>
        {#if topologyLab?.near_cutoff_pairs?.length}
          <div class="pair-table">
            {#each topologyLab.near_cutoff_pairs.slice(0, 8) as pair}
              <div class="pair-row">
                <strong>{pair.left_species}{pair.left} - {pair.right_species}{pair.right}</strong>
                <span>{pair.distance.toFixed(3)} A</span>
                <span>{pair.margin.toFixed(3)} A</span>
                <span>{pair.is_edge ? 'edge' : 'gap'}</span>
              </div>
            {/each}
          </div>
        {:else}
          <div class="placeholder">Pair-margin evidence will appear here.</div>
        {/if}
      </article>

      <article class="panel panel-wide">
        <div class="panel-head">
          <h2>Dreadnaut graph program</h2>
          <span>exact canonicalization input</span>
        </div>
        <pre>{topologyLab?.graph_text || 'Awaiting topology payload.'}</pre>
      </article>

      <article class="panel panel-wide">
        <div class="panel-head">
          <h2>Topology capabilities</h2>
          <span>what the codebase already suggests</span>
        </div>
        <ul class="vision-list">
          {#each topologyCapabilities as item}
            <li>{item}</li>
          {/each}
        </ul>
      </article>
    </section>
  {/if}

</div>
