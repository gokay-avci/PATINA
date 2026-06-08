use anyhow::{Context, Result};
use patina_search::BhStepTrace;
use std::fs;
use std::io::Write;
use std::path::Path;

pub fn write_rust_bh_walker_trace_csv(run_dir: &Path, trace: &[BhStepTrace]) -> Result<()> {
    let traces_dir = run_dir.join("traces");
    fs::create_dir_all(&traces_dir)
        .with_context(|| format!("failed to create `{}`", traces_dir.display()))?;
    let path = traces_dir.join("walker_trace.csv");
    let mut file = fs::File::create(&path)
        .with_context(|| format!("failed to create `{}`", path.display()))?;
    writeln!(
        file,
        "step,walker_id,accepted,energy,best_energy,temperature,step_size,move_class,label,reason,matched_relaxation_level,energy_source"
    )
    .context("failed to write walker trace header")?;

    for row in trace {
        let energy = row
            .energy
            .map(|value| format!("{value:.10}"))
            .unwrap_or_default();
        let best_energy = row
            .best_energy
            .map(|value| format!("{value:.10}"))
            .unwrap_or_default();
        writeln!(
            file,
            "{},{},{},{},{},{:.10},{:.10},{},{},{},{},{}",
            row.step,
            row.walker_id,
            row.accepted,
            energy,
            best_energy,
            row.temperature,
            row.step_size,
            bh_move_class_label(row.move_class),
            sanitize_csv_field(&row.label),
            sanitize_csv_field(&row.reason),
            row.matched_relaxation_level
                .map(|level| level.to_string())
                .unwrap_or_default(),
            if row.energy.is_some() {
                "rust_eval"
            } else {
                "undefined"
            },
        )
        .context("failed to write walker trace row")?;
    }

    Ok(())
}

pub fn bh_move_class_label(move_class: patina_search::BhMoveClass) -> &'static str {
    match move_class {
        patina_search::BhMoveClass::MonteCarlo => "monte_carlo",
        patina_search::BhMoveClass::SwapCations => "swap_cations",
        patina_search::BhMoveClass::SwapAtoms => "swap_atoms",
        patina_search::BhMoveClass::MutateCluster => "mutate_cluster",
        patina_search::BhMoveClass::TwistCluster => "twist_cluster",
        patina_search::BhMoveClass::TranslateCluster => "translate_cluster",
        patina_search::BhMoveClass::RotateCluster => "rotate_cluster",
    }
}

fn sanitize_csv_field(value: &str) -> String {
    value.replace(',', ";")
}

#[cfg(test)]
mod tests {
    use super::{bh_move_class_label, write_rust_bh_walker_trace_csv};
    use patina_search::{BhMoveClass, BhStepTrace};

    #[test]
    fn move_class_labels_match_trace_contract() {
        assert_eq!(bh_move_class_label(BhMoveClass::MonteCarlo), "monte_carlo");
        assert_eq!(
            bh_move_class_label(BhMoveClass::SwapCations),
            "swap_cations"
        );
        assert_eq!(bh_move_class_label(BhMoveClass::SwapAtoms), "swap_atoms");
        assert_eq!(
            bh_move_class_label(BhMoveClass::MutateCluster),
            "mutate_cluster"
        );
        assert_eq!(
            bh_move_class_label(BhMoveClass::TwistCluster),
            "twist_cluster"
        );
        assert_eq!(
            bh_move_class_label(BhMoveClass::TranslateCluster),
            "translate_cluster"
        );
        assert_eq!(
            bh_move_class_label(BhMoveClass::RotateCluster),
            "rotate_cluster"
        );
    }

    #[test]
    fn writes_rust_bh_walker_trace_with_native_comparable_columns() {
        let dir = tempfile::tempdir().expect("tempdir");
        let trace = vec![BhStepTrace {
            step: 2,
            walker_id: 0,
            accepted: true,
            energy: Some(-12.5),
            best_energy: Some(-12.5),
            temperature: 50.0,
            step_size: 0.2,
            move_class: BhMoveClass::SwapAtoms,
            label: "bh_step_0002_0000".into(),
            reason: "accepted".into(),
            matched_relaxation_level: Some(2),
        }];

        write_rust_bh_walker_trace_csv(dir.path(), &trace).expect("write trace");

        let content = std::fs::read_to_string(dir.path().join("traces").join("walker_trace.csv"))
            .expect("read trace");
        let expected = include_str!("fixtures/bh/rust_matched_relaxation_trace.csv");
        assert_eq!(content, expected);
    }
}
