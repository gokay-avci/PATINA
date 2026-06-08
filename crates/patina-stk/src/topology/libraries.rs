/*!
Topology library landing zone.

Priority order for the current campaign:

- `cage`
- `metal_complex`
- `macrocycle`
- `polymer`
- `rotaxane`
- `host_guest`

The first usable implementations remain non-periodic and 0D-first.
 */

pub mod cage;
pub mod host_guest;
pub mod macrocycle;
pub mod metal_complex;
pub mod polymer;
pub mod rotaxane;
pub mod zero_d;
