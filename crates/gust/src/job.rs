use std::time::Duration;

/// Backend-neutral state of an asynchronous GPU submission.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum JobStatus {
    Pending,
    Complete,
}

impl JobStatus {
    pub const fn is_complete(self) -> bool {
        matches!(self, Self::Complete)
    }
}

/// Backend-neutral completion contract for submitted GPU work.
pub trait Job {
    type Error;

    /// Poll once without blocking.
    fn status(&self) -> Result<JobStatus, Self::Error>;

    /// Wait for at most `timeout` and return the resulting state.
    fn wait_timeout(&self, timeout: Duration) -> Result<JobStatus, Self::Error>;

    /// Wait until the submission has completed.
    fn wait(&self) -> Result<(), Self::Error>;

    fn is_complete(&self) -> Result<bool, Self::Error> {
        self.status().map(JobStatus::is_complete)
    }
}

/// Backend-neutral device contract for dispatch descriptions and asynchronous jobs.
pub trait Device {
    type Error;
    type Dispatch<'resources>;
    type Job<'device>: Job<Error = Self::Error>
    where
        Self: 'device;

    fn submit<'device, 'resources>(
        &'device self,
        dispatches: &[Self::Dispatch<'resources>],
    ) -> Result<Self::Job<'device>, Self::Error>;

    fn dispatch<'resources>(
        &self,
        dispatches: &[Self::Dispatch<'resources>],
    ) -> Result<(), Self::Error> {
        self.submit(dispatches)?.wait()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn completion_state_is_explicit() {
        assert!(!JobStatus::Pending.is_complete());
        assert!(JobStatus::Complete.is_complete());
    }
}
