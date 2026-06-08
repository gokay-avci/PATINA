use serde::{Deserialize, Serialize};

/// Runtime-visible lifecycle of a leased Scott job.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LeaseState {
    Offered,
    Accepted,
    Active,
    Completed,
    Cancelled,
    Reclaimed,
    Expired,
}

/// Short-lived lease granted by the orchestration layer to a worker or allocation.
///
/// This is the scheduler-facing control-plane concept that is missing from the legacy MPI
/// taskfarm. A lease tracks who currently owns the right to stage and execute the Scott job.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorkLease {
    pub lease_id: String,
    pub job_id: String,
    pub worker_id: String,
    pub issued_at_ms: u64,
    pub expires_at_ms: u64,
    pub state: LeaseState,
}

impl WorkLease {
    pub fn is_expired_at(&self, now_ms: u64) -> bool {
        now_ms >= self.expires_at_ms
    }

    pub fn renew_until(&mut self, expires_at_ms: u64) {
        self.expires_at_ms = expires_at_ms;
        if matches!(self.state, LeaseState::Accepted | LeaseState::Offered) {
            self.state = LeaseState::Active;
        }
    }

    pub fn reclaim(&mut self) {
        self.state = LeaseState::Reclaimed;
    }
}

#[cfg(test)]
mod tests {
    use super::{LeaseState, WorkLease};

    #[test]
    fn lease_detects_expiry() {
        let lease = WorkLease {
            lease_id: "lease-1".into(),
            job_id: "job-1".into(),
            worker_id: "worker-a".into(),
            issued_at_ms: 100,
            expires_at_ms: 200,
            state: LeaseState::Accepted,
        };

        assert!(!lease.is_expired_at(199));
        assert!(lease.is_expired_at(200));
    }

    #[test]
    fn lease_renewal_promotes_to_active() {
        let mut lease = WorkLease {
            lease_id: "lease-2".into(),
            job_id: "job-2".into(),
            worker_id: "worker-b".into(),
            issued_at_ms: 100,
            expires_at_ms: 200,
            state: LeaseState::Accepted,
        };

        lease.renew_until(400);
        assert_eq!(lease.expires_at_ms, 400);
        assert_eq!(lease.state, LeaseState::Active);
    }
}
