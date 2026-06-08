/*!
Periodic topology helpers landing zone.
*/

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct PeriodicShift {
    pub shift: [i32; 3],
}

impl PeriodicShift {
    pub fn new(shift: [i32; 3]) -> Self {
        Self { shift }
    }

    pub fn is_zero(self) -> bool {
        self.shift.iter().all(|value| *value == 0)
    }
}

pub fn lattice_shift_cartesian(lattice: [[f64; 3]; 3], shift: [i32; 3]) -> [f64; 3] {
    let mut result = [0.0; 3];
    for axis in 0..3 {
        let scale = shift[axis] as f64;
        result[0] += lattice[axis][0] * scale;
        result[1] += lattice[axis][1] * scale;
        result[2] += lattice[axis][2] * scale;
    }
    result
}
