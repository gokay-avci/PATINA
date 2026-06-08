use anyhow::{Context, Result};
use patina_search::McStepTrace;
use std::fs;
use std::io::Write;
use std::path::Path;

pub fn write_rust_mc_trace_csv(run_dir: &Path, trace: &[McStepTrace]) -> Result<()> {
    let traces_dir = run_dir.join("traces");
    fs::create_dir_all(&traces_dir)
        .with_context(|| format!("failed to create `{}`", traces_dir.display()))?;
    let path = traces_dir.join("mc_trace.csv");
    let mut file = fs::File::create(&path)
        .with_context(|| format!("failed to create `{}`", path.display()))?;
    writeln!(
        file,
        "step,accepted,energy,best_energy,temperature,threshold,label,reason,energy_source"
    )
    .context("failed to write mc trace header")?;

    for row in trace {
        let energy = row
            .energy
            .map(|value| format!("{value:.10}"))
            .unwrap_or_default();
        let best_energy = row
            .best_energy
            .map(|value| format!("{value:.10}"))
            .unwrap_or_default();
        let threshold = row
            .threshold
            .map(|value| format!("{value:.10}"))
            .unwrap_or_default();
        writeln!(
            file,
            "{},{},{},{},{:.10},{},{},{},{}",
            row.step,
            row.accepted,
            energy,
            best_energy,
            row.temperature,
            threshold,
            sanitize_csv_field(&row.label),
            sanitize_csv_field(&row.reason),
            if row.energy.is_some() {
                "rust_eval"
            } else {
                "undefined"
            },
        )
        .context("failed to write mc trace row")?;
    }

    Ok(())
}

fn sanitize_csv_field(value: &str) -> String {
    value.replace(',', ";")
}

#[cfg(test)]
mod tests {
    use super::write_rust_mc_trace_csv;
    use patina_search::McStepTrace;

    #[test]
    fn writes_rust_mc_trace_with_native_comparable_columns() {
        let dir = tempfile::tempdir().expect("tempdir");
        let trace = vec![McStepTrace {
            step: 1,
            accepted: true,
            energy: Some(-12.5),
            best_energy: Some(-12.5),
            temperature: 50.0,
            threshold: Some(-11.0),
            label: "mc_step_0001".into(),
            reason: "accepted".into(),
        }];

        write_rust_mc_trace_csv(dir.path(), &trace).expect("write trace");

        let content = std::fs::read_to_string(dir.path().join("traces").join("mc_trace.csv"))
            .expect("read trace");
        let mut lines = content.lines();
        assert_eq!(
            lines.next(),
            Some(
                "step,accepted,energy,best_energy,temperature,threshold,label,reason,energy_source"
            )
        );
        assert_eq!(
            lines.next(),
            Some("1,true,-12.5000000000,-12.5000000000,50.0000000000,-11.0000000000,mc_step_0001,accepted,rust_eval")
        );
    }
}
