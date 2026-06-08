use crate::domain::{MotifCandidate, TopologyResult};
use patina_sci_kernel::codec::xyz::{write_xyz_frames, AtomRecord, XyzFrame};
use std::fs;
use std::path::{Path, PathBuf};

pub fn xyz_frame(candidate: &MotifCandidate) -> XyzFrame {
    let mut comment = format!(
        "id={} formula={} generator={}",
        candidate.id.0,
        candidate.composition.total_formula(),
        candidate.generator.name
    );
    for (key, value) in &candidate.parameters {
        if value.is_string() || value.is_number() || value.is_boolean() {
            comment.push(' ');
            comment.push_str(key);
            comment.push('=');
            comment.push_str(value.as_str().unwrap_or(&value.to_string()));
        }
    }
    if let Some(signature) = &candidate.topology_signature {
        comment.push_str(" fast_hash=");
        comment.push_str(&signature.hashes.fast_hash);
    }
    XyzFrame {
        atom_count: candidate.atoms.len(),
        comment,
        atoms: candidate
            .atoms
            .iter()
            .map(|atom| AtomRecord {
                species: atom.element.to_string(),
                coords: atom.position,
            })
            .collect(),
    }
}

pub fn write_candidate_xyz(
    path: impl AsRef<Path>,
    candidate: &MotifCandidate,
) -> TopologyResult<()> {
    write_xyz_frames(path.as_ref(), &[xyz_frame(candidate)]).map_err(|err| {
        crate::domain::TopologyError::Io {
            message: err.to_string(),
        }
    })
}

pub fn write_xyz_directory(
    out_dir: impl AsRef<Path>,
    candidates: &[MotifCandidate],
) -> TopologyResult<()> {
    let out_dir = out_dir.as_ref();
    fs::create_dir_all(out_dir)?;
    for candidate in candidates {
        let path = out_dir.join(format!("{}.xyz", candidate.id.0));
        write_candidate_xyz(path, candidate)?;
    }
    Ok(())
}

pub fn write_classified_xyz_directory<'a>(
    out_dir: impl AsRef<Path>,
    classified: impl IntoIterator<Item = (&'a MotifCandidate, String)>,
) -> TopologyResult<usize> {
    let out_dir = out_dir.as_ref();
    fs::create_dir_all(out_dir)?;
    let mut count = 0usize;
    for (candidate, class_name) in classified {
        let class_dir = out_dir.join(sanitize_path_component(&class_name));
        fs::create_dir_all(&class_dir)?;
        write_candidate_xyz(class_dir.join(format!("{}.xyz", candidate.id.0)), candidate)?;
        count += 1;
    }
    Ok(count)
}

pub fn classified_xyz_path(
    out_dir: impl AsRef<Path>,
    class_name: &str,
    candidate: &MotifCandidate,
) -> PathBuf {
    out_dir
        .as_ref()
        .join(sanitize_path_component(class_name))
        .join(format!("{}.xyz", candidate.id.0))
}

pub fn sanitize_path_component(value: &str) -> String {
    let sanitized = value
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() || ch == '-' || ch == '_' || ch == '.' {
                ch
            } else {
                '_'
            }
        })
        .collect::<String>();
    if sanitized.is_empty() {
        "unknown".to_string()
    } else {
        sanitized
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::Composition;
    use crate::generators::RingGenerator;

    #[test]
    fn xyz_line_count_matches_atom_count_plus_two() {
        let candidate = RingGenerator::new(1.5)
            .generate(1, Composition::from_formula("AB", 2).unwrap(), Some(1))
            .unwrap();
        let frame = xyz_frame(&candidate);
        let encoded = patina_sci_kernel::codec::xyz::encode_xyz_frames(&[frame]).unwrap();
        assert_eq!(encoded.lines().count(), candidate.atoms.len() + 2);
    }
}
