/*!
External optimizer port landing zone.
*/

use crate::domain::StkDomainError;

pub trait ExternalOptimizerPort {
    fn status(&self) -> Result<&'static str, StkDomainError>;
}
