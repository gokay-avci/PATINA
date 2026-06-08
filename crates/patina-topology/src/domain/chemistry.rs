use crate::domain::{Composition, TopologyError, TopologyResult};
use patina_sci_kernel::HashkeyRadiusMode;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BondLengthEstimate {
    pub bond_length: f64,
    pub mode: BondLengthMode,
    pub source: String,
    pub pair: Option<(String, String)>,
    pub message: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BondLengthMode {
    Ionic,
    Covalent,
}

impl BondLengthMode {
    pub fn parse(value: &str) -> TopologyResult<Self> {
        match value.to_ascii_lowercase().replace('_', "-").as_str() {
            "ionic" | "ir" => Ok(Self::Ionic),
            "covalent" | "cr" => Ok(Self::Covalent),
            other => Err(TopologyError::UnsupportedBondLengthMode {
                mode: other.to_string(),
            }),
        }
    }

    pub fn radius_mode(self) -> HashkeyRadiusMode {
        match self {
            Self::Ionic => HashkeyRadiusMode::Ionic,
            Self::Covalent => HashkeyRadiusMode::Covalent,
        }
    }
}

pub fn estimate_bond_length_from_formula(
    formula: &str,
    mode: BondLengthMode,
    radius_const: f64,
    fallback: f64,
) -> TopologyResult<BondLengthEstimate> {
    let composition = Composition::from_formula(formula, 1)?;
    let species = composition
        .formula_unit
        .counts
        .keys()
        .map(|element| element.to_string())
        .collect::<Vec<_>>();
    let atom_specs = match patina_dreadnaut::builtin_atom_specs() {
        Ok(specs) => specs,
        Err(error) => {
            return Ok(fallback_estimate(
                fallback,
                mode,
                format!("built-in atom-spec lookup failed: {error}"),
            ));
        }
    };

    let mut best: Option<(String, String, f64)> = None;
    for (left_index, left) in species.iter().enumerate() {
        for (right_index, right) in species.iter().enumerate() {
            if species.len() > 1 && left_index == right_index {
                continue;
            }
            if species.len() == 1 && right_index != left_index {
                continue;
            }
            let Some(left_radius) = species_radius(left, &atom_specs, mode) else {
                return Ok(fallback_estimate(
                    fallback,
                    mode,
                    format!("no built-in atom specification for species `{left}`"),
                ));
            };
            let Some(right_radius) = species_radius(right, &atom_specs, mode) else {
                return Ok(fallback_estimate(
                    fallback,
                    mode,
                    format!("no built-in atom specification for species `{right}`"),
                ));
            };
            let distance = left_radius + right_radius + radius_const;
            if distance <= 0.0 {
                continue;
            }
            if best
                .as_ref()
                .is_none_or(|(_, _, best_distance)| distance > *best_distance)
            {
                best = Some((left.clone(), right.clone(), distance));
            }
        }
    }

    let Some((left, right, bond_length)) = best else {
        return Ok(fallback_estimate(
            fallback,
            mode,
            "no positive radius-pair estimate was available".to_string(),
        ));
    };
    Ok(BondLengthEstimate {
        bond_length,
        mode,
        source: "patina-dreadnaut builtin_atom_specs.csv".to_string(),
        pair: Some(canonical_pair(left, right)),
        message: "estimated as species radius sum plus radius constant".to_string(),
    })
}

fn species_radius(
    species: &str,
    atom_specs: &[patina_dreadnaut::AtomSpec],
    mode: BondLengthMode,
) -> Option<f64> {
    let spec = atom_specs.iter().find(|spec| spec.species == species)?;
    let radius = match mode.radius_mode() {
        HashkeyRadiusMode::Ionic => spec.ionic_radius,
        HashkeyRadiusMode::Covalent => spec.covalent_radius,
    };
    (radius > 0.0).then_some(radius)
}

fn fallback_estimate(fallback: f64, mode: BondLengthMode, message: String) -> BondLengthEstimate {
    BondLengthEstimate {
        bond_length: fallback,
        mode,
        source: "fallback".to_string(),
        pair: None,
        message,
    }
}

fn canonical_pair(left: String, right: String) -> (String, String) {
    if left <= right {
        (left, right)
    } else {
        (right, left)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mgo_ionic_bond_length_uses_builtin_radii() {
        let estimate =
            estimate_bond_length_from_formula("MgO", BondLengthMode::Ionic, 0.0, 1.5).unwrap();
        assert!((estimate.bond_length - 2.12).abs() < 1.0e-12);
        assert_eq!(estimate.pair, Some(("Mg".to_string(), "O".to_string())));
    }

    #[test]
    fn generic_formula_falls_back() {
        let estimate =
            estimate_bond_length_from_formula("AB2", BondLengthMode::Ionic, 0.0, 1.5).unwrap();
        assert_eq!(estimate.bond_length, 1.5);
        assert_eq!(estimate.source, "fallback");
    }
}
