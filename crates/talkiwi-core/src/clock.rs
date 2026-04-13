use std::sync::Arc;
use std::time::Instant;

/// Monotonic clock shared by all tracks in a session.
#[derive(Debug, Clone)]
pub struct SessionClock {
    origin: Arc<Instant>,
}

impl SessionClock {
    pub fn new() -> Self {
        Self {
            origin: Arc::new(Instant::now()),
        }
    }

    pub fn elapsed_ms(&self) -> u64 {
        self.origin.elapsed().as_millis() as u64
    }

    pub fn origin(&self) -> Instant {
        *self.origin
    }
}

impl Default for SessionClock {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::SessionClock;

    #[test]
    fn session_clock_elapsed_is_monotonic() {
        let clock = SessionClock::new();
        let first = clock.elapsed_ms();
        std::thread::sleep(std::time::Duration::from_millis(5));
        let second = clock.elapsed_ms();
        assert!(second >= first);
    }
}
