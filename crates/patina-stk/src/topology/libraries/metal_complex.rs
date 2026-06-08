/*!
0D metal-complex topology definitions.
*/

use crate::topology::libraries::zero_d::{square_planar_zero_d_plan, ZeroDTopologyPlan};

pub fn square_planar_plan() -> ZeroDTopologyPlan {
    square_planar_zero_d_plan()
}

#[cfg(test)]
mod tests {
    use super::square_planar_plan;

    #[test]
    fn square_planar_plan_constructs() {
        let result = square_planar_plan()
            .construct_with_default_driver()
            .expect("construct square planar complex");
        assert_eq!(result.molecule_state().bonds().len(), 4);
    }
}
