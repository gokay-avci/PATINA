use crate::domain::{TopologyError, TopologyResult};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::fmt;
use std::str::FromStr;

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct ElementSymbol(pub String);

impl ElementSymbol {
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for ElementSymbol {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl From<&str> for ElementSymbol {
    fn from(value: &str) -> Self {
        Self(value.to_string())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FormulaUnit {
    pub counts: BTreeMap<ElementSymbol, usize>,
    pub order: Vec<ElementSymbol>,
}

impl FormulaUnit {
    pub fn parse(input: &str) -> TopologyResult<Self> {
        if input.trim().is_empty() {
            return Err(TopologyError::EmptyFormula);
        }
        let formula = input.trim();
        let chars = formula.char_indices().collect::<Vec<_>>();
        let mut cursor = 0usize;
        let mut counts = BTreeMap::<ElementSymbol, usize>::new();
        let mut order = Vec::<ElementSymbol>::new();

        while cursor < chars.len() {
            let (start, ch) = chars[cursor];
            if !ch.is_ascii_uppercase() {
                return Err(TopologyError::InvalidFormula {
                    formula: formula.to_string(),
                    index: start,
                });
            }
            cursor += 1;
            let mut symbol = String::new();
            symbol.push(ch);
            if cursor < chars.len() && chars[cursor].1.is_ascii_lowercase() {
                symbol.push(chars[cursor].1);
                cursor += 1;
            }

            let number_start = cursor;
            while cursor < chars.len() && chars[cursor].1.is_ascii_digit() {
                cursor += 1;
            }
            let count = if number_start == cursor {
                1
            } else {
                formula[chars[number_start].0..chars[cursor - 1].0 + chars[cursor - 1].1.len_utf8()]
                    .parse::<usize>()
                    .map_err(|_| TopologyError::InvalidFormula {
                        formula: formula.to_string(),
                        index: chars[number_start].0,
                    })?
            };
            if count == 0 {
                return Err(TopologyError::InvalidFormula {
                    formula: formula.to_string(),
                    index: start,
                });
            }
            let element = ElementSymbol(symbol);
            if !counts.contains_key(&element) {
                order.push(element.clone());
            }
            *counts.entry(element).or_insert(0) += count;
        }

        Ok(Self { counts, order })
    }

    pub fn atoms_per_unit(&self) -> usize {
        self.counts.values().sum()
    }

    pub fn ordered_elements(&self) -> &[ElementSymbol] {
        &self.order
    }

    pub fn format_counts(&self, counts: &BTreeMap<ElementSymbol, usize>) -> String {
        let mut rendered = String::new();
        for element in &self.order {
            if let Some(count) = counts.get(element) {
                rendered.push_str(element.as_str());
                if *count != 1 {
                    rendered.push_str(&count.to_string());
                }
            }
        }
        rendered
    }
}

impl fmt::Display for FormulaUnit {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.format_counts(&self.counts))
    }
}

impl FromStr for FormulaUnit {
    type Err = TopologyError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Self::parse(s)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Composition {
    pub formula_unit: FormulaUnit,
    pub n_formula_units: usize,
    pub total_counts: BTreeMap<ElementSymbol, usize>,
    pub total_atoms: usize,
}

impl Composition {
    pub fn new(formula_unit: FormulaUnit, n_formula_units: usize) -> TopologyResult<Self> {
        if n_formula_units == 0 {
            return Err(TopologyError::InvalidFormulaUnitCount);
        }
        let total_counts = formula_unit
            .counts
            .iter()
            .map(|(element, count)| (element.clone(), count * n_formula_units))
            .collect::<BTreeMap<_, _>>();
        let total_atoms = total_counts.values().sum();
        Ok(Self {
            formula_unit,
            n_formula_units,
            total_counts,
            total_atoms,
        })
    }

    pub fn from_formula(formula: &str, n_formula_units: usize) -> TopologyResult<Self> {
        Self::new(FormulaUnit::parse(formula)?, n_formula_units)
    }

    pub fn total_formula(&self) -> String {
        self.formula_unit.format_counts(&self.total_counts)
    }

    pub fn ordered_atom_labels(&self) -> Vec<ElementSymbol> {
        let mut labels = Vec::with_capacity(self.total_atoms);
        for _ in 0..self.n_formula_units {
            for element in self.formula_unit.ordered_elements() {
                let count = self.formula_unit.counts.get(element).copied().unwrap_or(0);
                labels.extend(std::iter::repeat_n(element.clone(), count));
            }
        }
        labels
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_generic_and_real_formulae() {
        let ti = FormulaUnit::parse("Ti3N4").unwrap();
        assert_eq!(ti.counts.get(&ElementSymbol::from("Ti")), Some(&3));
        assert_eq!(ti.counts.get(&ElementSymbol::from("N")), Some(&4));
        assert_eq!(ti.to_string(), "Ti3N4");

        assert_eq!(FormulaUnit::parse("MgO").unwrap().to_string(), "MgO");
        assert_eq!(FormulaUnit::parse("AB2").unwrap().to_string(), "AB2");
        assert_eq!(FormulaUnit::parse("A2B3").unwrap().to_string(), "A2B3");
    }

    #[test]
    fn scales_composition() {
        let composition = Composition::from_formula("Ti3N4", 4).unwrap();
        assert_eq!(composition.total_atoms, 28);
        assert_eq!(composition.total_formula(), "Ti12N16");
    }
}
