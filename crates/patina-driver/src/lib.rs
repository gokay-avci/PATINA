pub mod application {
    #[path = "ga_emulate.rs"]
    pub mod ga_emulate;
    #[path = "ga_execution.rs"]
    pub mod ga_execution;
    #[path = "hybrid_core.rs"]
    pub mod hybrid_core;

    pub mod ports {
        #[path = "framework.rs"]
        mod framework;
        #[path = "ga.rs"]
        mod ga;
        #[path = "hybrid.rs"]
        mod hybrid;
        #[path = "production.rs"]
        mod production;
        #[path = "surface.rs"]
        mod surface;

        pub use framework::*;
        pub use ga::*;
        pub use hybrid::*;
        pub use production::*;
        pub use surface::*;
    }

    #[path = "framework_gcmc.rs"]
    pub mod framework_gcmc;
    #[path = "framework_normalization.rs"]
    pub mod framework_normalization;
    #[path = "framework_symmetry.rs"]
    pub mod framework_symmetry;
    #[path = "scott_production.rs"]
    pub mod scott_production;
    #[path = "scott_topology_types.rs"]
    pub mod scott_topology_types;
    #[path = "surface_generation.rs"]
    pub mod surface_generation;
    #[path = "surface_polarity.rs"]
    pub mod surface_polarity;
    #[path = "topology_identity.rs"]
    pub mod topology_identity;
    #[path = "workflow_tasks.rs"]
    pub mod workflow_tasks;
}

pub use application::scott_topology_types::AtomSpecRecord;
