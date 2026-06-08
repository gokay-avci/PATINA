const SYMBOLS: [&str; 100] = [
    "H", "He", "Li", "Be", "B", "C", "N", "O", "F", "Ne", "Na", "Mg", "Al", "Si", "P", "S", "Cl",
    "Ar", "K", "Ca", "Sc", "Ti", "V", "Cr", "Mn", "Fe", "Co", "Ni", "Cu", "Zn", "Ga", "Ge", "As",
    "Se", "Br", "Kr", "Rb", "Sr", "Y", "Zr", "Nb", "Mo", "Tc", "Ru", "Rh", "Pd", "Ag", "Cd", "In",
    "Sn", "Sb", "Te", "I", "Xe", "Cs", "Ba", "La", "Ce", "Pr", "Nd", "Pm", "Sm", "Eu", "Gd", "Tb",
    "Dy", "Ho", "Er", "Tm", "Yb", "Lu", "Hf", "Ta", "W", "Re", "Os", "Ir", "Pt", "Au", "Hg", "Tl",
    "Pb", "Bi", "Po", "At", "Rn", "Fr", "Ra", "Ac", "Th", "Pa", "U", "Np", "Pu", "Am", "Cm", "Bk",
    "Cf", "Es", "Fm",
];

const LEGACY_MASSES: [Option<f64>; 100] = [
    Some(1.00783),
    Some(4.00260),
    Some(6.94000),
    Some(9.01218),
    Some(10.81000),
    Some(12.00000),
    Some(14.00307),
    Some(15.99491),
    Some(18.99840),
    Some(20.17900),
    Some(22.98977),
    Some(24.30500),
    Some(26.98154),
    Some(28.08550),
    Some(30.97376),
    Some(32.06000),
    Some(35.45300),
    Some(39.94800),
    Some(39.09830),
    Some(40.08000),
    Some(44.95590),
    Some(47.90000),
    Some(50.94150),
    Some(51.99600),
    Some(54.93800),
    Some(55.84700),
    Some(58.93320),
    Some(58.71000),
    Some(63.54600),
    Some(65.38000),
    Some(69.73500),
    Some(72.59000),
    Some(74.92160),
    Some(78.96000),
    Some(79.90400),
    Some(83.80000),
    Some(85.46780),
    Some(87.62000),
    Some(88.90590),
    Some(91.22000),
    Some(92.90640),
    Some(95.94000),
    Some(98.90620),
    Some(101.0700),
    Some(102.9055),
    Some(106.4000),
    Some(107.8680),
    Some(112.4100),
    Some(114.8200),
    Some(118.6900),
    Some(121.7500),
    Some(127.6000),
    Some(126.9045),
    Some(131.3000),
    Some(132.9054),
    Some(137.3300),
    None,
    None,
    None,
    None,
    None,
    None,
    None,
    None,
    None,
    None,
    None,
    None,
    None,
    None,
    None,
    Some(178.4900),
    Some(180.9479),
    Some(183.8500),
    Some(186.2070),
    Some(190.2000),
    Some(192.2200),
    Some(194.9648),
    Some(196.9665),
    Some(200.5900),
    Some(204.3700),
    Some(207.2000),
    Some(208.9804),
    None,
    None,
    None,
    None,
    None,
    None,
    None,
    None,
    None,
    None,
    None,
    None,
    None,
    None,
    None,
    None,
    None,
];

pub fn canonicalize_species_symbol(species: &str) -> Option<String> {
    let trimmed = species.trim();
    if trimmed.is_empty()
        || trimmed.len() > 2
        || !trimmed.chars().all(|ch| ch.is_ascii_alphabetic())
    {
        return None;
    }

    let mut chars = trimmed.chars();
    let first = chars.next()?.to_ascii_uppercase();
    let second = chars.next().map(|ch| ch.to_ascii_lowercase());
    let canonical = match second {
        Some(second) => format!("{first}{second}"),
        None => first.to_string(),
    };

    atomic_number_for_symbol(&canonical).map(|_| canonical)
}

pub fn atomic_number_for_symbol(species: &str) -> Option<u8> {
    let canonical = canonicalize_without_lookup(species)?;
    SYMBOLS
        .iter()
        .position(|symbol| *symbol == canonical)
        .map(|index| (index + 1) as u8)
}

pub fn symbol_for_atomic_number(atomic_number: u8) -> Option<&'static str> {
    let index = atomic_number.checked_sub(1)? as usize;
    SYMBOLS.get(index).copied()
}

pub fn legacy_atomic_mass(atomic_number: u8) -> Option<f64> {
    let index = atomic_number.checked_sub(1)? as usize;
    LEGACY_MASSES.get(index).copied().flatten()
}

fn canonicalize_without_lookup(species: &str) -> Option<String> {
    let trimmed = species.trim();
    if trimmed.is_empty()
        || trimmed.len() > 2
        || !trimmed.chars().all(|ch| ch.is_ascii_alphabetic())
    {
        return None;
    }

    let mut chars = trimmed.chars();
    let first = chars.next()?.to_ascii_uppercase();
    let second = chars.next().map(|ch| ch.to_ascii_lowercase());
    Some(match second {
        Some(second) => format!("{first}{second}"),
        None => first.to_string(),
    })
}

#[cfg(test)]
mod tests {
    use super::{
        atomic_number_for_symbol, canonicalize_species_symbol, legacy_atomic_mass,
        symbol_for_atomic_number,
    };

    #[test]
    fn supports_full_symbol_lookup_to_element_100() {
        assert_eq!(symbol_for_atomic_number(1), Some("H"));
        assert_eq!(symbol_for_atomic_number(79), Some("Au"));
        assert_eq!(symbol_for_atomic_number(90), Some("Th"));
        assert_eq!(symbol_for_atomic_number(100), Some("Fm"));
        assert_eq!(symbol_for_atomic_number(101), None);
    }

    #[test]
    fn normalizes_symbol_casing() {
        assert_eq!(canonicalize_species_symbol(" mg "), Some("Mg".into()));
        assert_eq!(canonicalize_species_symbol("cl"), Some("Cl".into()));
        assert_eq!(atomic_number_for_symbol("FM"), Some(100));
        assert_eq!(atomic_number_for_symbol("xx"), None);
    }

    #[test]
    fn preserves_legacy_mass_table_gaps() {
        assert_eq!(legacy_atomic_mass(6), Some(12.0));
        assert_eq!(legacy_atomic_mass(57), None);
        assert_eq!(legacy_atomic_mass(78), Some(194.9648));
        assert_eq!(legacy_atomic_mass(90), None);
        assert_eq!(legacy_atomic_mass(92), None);
    }
}
