use crate::domain::{MotifCandidate, TopologyError, TopologyResult};
use crate::fingerprints::SignatureRecord;
use std::fs::File;
use std::io::{BufRead, BufReader, BufWriter, Write};
use std::path::Path;

pub fn write_candidates_jsonl(
    path: impl AsRef<Path>,
    candidates: &[MotifCandidate],
) -> TopologyResult<()> {
    let file = File::create(path)?;
    let mut writer = BufWriter::new(file);
    for candidate in candidates {
        serde_json::to_writer(&mut writer, candidate)?;
        writer.write_all(b"\n")?;
    }
    Ok(())
}

pub fn read_candidates_jsonl(path: impl AsRef<Path>) -> TopologyResult<Vec<MotifCandidate>> {
    let file = File::open(path)?;
    let reader = BufReader::new(file);
    let mut out = Vec::new();
    for (line_number, line) in reader.lines().enumerate() {
        let line = line?;
        if line.trim().is_empty() {
            continue;
        }
        let candidate =
            serde_json::from_str::<MotifCandidate>(&line).map_err(|err| TopologyError::Json {
                message: format!("line {}: {err}", line_number + 1),
            })?;
        out.push(candidate);
    }
    Ok(out)
}

pub fn write_signatures_jsonl(
    path: impl AsRef<Path>,
    records: &[SignatureRecord],
) -> TopologyResult<()> {
    let file = File::create(path)?;
    let mut writer = BufWriter::new(file);
    for record in records {
        serde_json::to_writer(&mut writer, record)?;
        writer.write_all(b"\n")?;
    }
    Ok(())
}

pub fn read_signatures_jsonl(path: impl AsRef<Path>) -> TopologyResult<Vec<SignatureRecord>> {
    let file = File::open(path)?;
    let reader = BufReader::new(file);
    let mut out = Vec::new();
    for (line_number, line) in reader.lines().enumerate() {
        let line = line?;
        if line.trim().is_empty() {
            continue;
        }
        let record =
            serde_json::from_str::<SignatureRecord>(&line).map_err(|err| TopologyError::Json {
                message: format!("line {}: {err}", line_number + 1),
            })?;
        out.push(record);
    }
    Ok(out)
}
