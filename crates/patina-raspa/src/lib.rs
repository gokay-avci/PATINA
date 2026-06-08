#![forbid(unsafe_code)]

mod error;
mod framework;
mod gcmc;
mod properties;
mod symmetry;

pub use error::RaspaInterfaceError;
pub use framework::{FrameworkAtom, PeriodicFramework, RaspaPeriodicity};
pub use gcmc::{
    GcmcConditions, GcmcEnergyEvaluator, GcmcEngine, GcmcMoveSchedule, GcmcRequest, GcmcResult,
    GcmcSummary, GcmcTracePoint, GuestAtom, RigidGuestGcmcEngine, RigidGuestTemplate,
};
pub use properties::{
    DensityGrid, DensityGridBinning, DensityGridNormalization, DensityGridSpec, EnergyHistogram,
    EnergyHistogramSpec, HistogramEngine, NumberHistogram, NumberHistogramSpec, PropertyGridEngine,
    StaticFrameworkPropertyEngine,
};
pub use symmetry::{
    MoyoSymmetryAnalyzer, SymmetryAnalysis, SymmetryAnalyzer, SymmetryOperation, SymmetryTolerance,
};

#[cfg(test)]
mod tests {
    use super::{
        DensityGridBinning, DensityGridNormalization, DensityGridSpec, EnergyHistogramSpec,
        HistogramEngine, MoyoSymmetryAnalyzer, NumberHistogramSpec, PeriodicFramework,
        PropertyGridEngine, RaspaInterfaceError, RaspaPeriodicity, StaticFrameworkPropertyEngine,
        SymmetryAnalyzer,
    };
    use patina_types::Candidate;

    fn periodic_candidate() -> Candidate {
        Candidate::fully_periodic(
            "diamond_like_si",
            vec!["Si".into(), "Si".into()],
            vec![[0.0, 0.0, 0.0], [0.5, 0.5, 0.5]],
            [[5.43, 0.0, 0.0], [0.0, 5.43, 0.0], [0.0, 0.0, 5.43]],
        )
    }

    #[test]
    fn periodic_framework_accepts_three_d_candidate() {
        let framework =
            PeriodicFramework::try_from_candidate(&periodic_candidate()).expect("framework");
        assert_eq!(framework.periodicity, RaspaPeriodicity::ThreeD);
        assert_eq!(framework.atom_count(), 2);
    }

    #[test]
    fn periodic_framework_rejects_zero_d_candidate() {
        let mut candidate = periodic_candidate();
        candidate.lattice = None;
        candidate.periodic_axes = [false, false, false];
        let error = PeriodicFramework::try_from_candidate(&candidate).expect_err("must reject");
        assert!(matches!(error, RaspaInterfaceError::NonPeriodicCandidate));
    }

    #[test]
    fn periodic_framework_rejects_partial_periodicity_for_now() {
        let mut candidate = periodic_candidate();
        candidate.periodic_axes = [true, true, false];
        let error = PeriodicFramework::try_from_candidate(&candidate).expect_err("must reject");
        assert!(matches!(
            error,
            RaspaInterfaceError::PartialPeriodicityUnsupported { .. }
        ));
    }

    #[test]
    fn moyo_analyzer_returns_symmetry_dataset_for_three_d_framework() {
        let framework =
            PeriodicFramework::try_from_candidate(&periodic_candidate()).expect("framework");
        let analysis = MoyoSymmetryAnalyzer
            .analyze(&framework, Default::default())
            .expect("symmetry analysis");

        assert_eq!(analysis.international_number, Some(229));
        assert!(!analysis.operations.is_empty());
        assert_eq!(analysis.orbits.len(), framework.atom_count());
        assert_eq!(analysis.wyckoff_letters.len(), framework.atom_count());
        assert!(analysis.standardized_framework.is_some());
        assert!(analysis.primitive_standardized_framework.is_some());
    }

    #[test]
    fn static_property_engine_builds_density_grid() {
        let framework =
            PeriodicFramework::try_from_candidate(&periodic_candidate()).expect("framework");
        let grid = StaticFrameworkPropertyEngine
            .sample_density_grid(
                &framework,
                &DensityGridSpec {
                    dimensions: [8, 8, 8],
                    sample_every: 1,
                    write_every: 10,
                    normalization: DensityGridNormalization::Max,
                    binning: DensityGridBinning::Equitable,
                    pseudo_atom_channels: vec!["Si".into()],
                },
            )
            .expect("grid");

        assert_eq!(grid.dimensions, [8, 8, 8]);
        assert_eq!(grid.channels, 1);
        assert_eq!(grid.samples, framework.atom_count());
        assert_eq!(grid.values.len(), 8 * 8 * 8);
        assert!(grid.values.iter().copied().fold(0.0_f64, f64::max) <= 1.0);
    }

    #[test]
    fn static_property_engine_builds_energy_histogram() {
        let framework =
            PeriodicFramework::try_from_candidate(&periodic_candidate()).expect("framework");
        let histogram = StaticFrameworkPropertyEngine
            .energy_histogram(
                &framework,
                &EnergyHistogramSpec {
                    number_of_bins: 32,
                    range: (-1.0e6, 1.0e6),
                    sample_every: 1,
                    write_every: 10,
                },
            )
            .expect("histogram");

        assert_eq!(histogram.total.len(), 32);
        assert_eq!(histogram.vdw.len(), 32);
        assert_eq!(histogram.coulomb.len(), 32);
        assert_eq!(histogram.polarization.len(), 32);
        assert!(histogram.total.iter().sum::<f64>() > 0.0);
    }

    #[test]
    fn static_property_engine_builds_number_histogram() {
        let framework =
            PeriodicFramework::try_from_candidate(&periodic_candidate()).expect("framework");
        let histogram = StaticFrameworkPropertyEngine
            .number_histogram(
                &framework,
                &NumberHistogramSpec {
                    lower_limit: 0,
                    upper_limit: 4,
                    sample_every: 1,
                    write_every: 10,
                },
            )
            .expect("number histogram");

        assert_eq!(histogram.per_component.len(), 1);
        assert_eq!(histogram.per_component[0].len(), 5);
        assert_eq!(histogram.per_component[0][2], 1.0);
    }
}
