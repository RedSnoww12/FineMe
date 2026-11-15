use chrono::{DateTime, Utc};
use fineme_application::ports::Clock;

/// Horloge réelle du système.
#[derive(Debug, Clone, Copy, Default)]
pub struct SystemClock;

impl Clock for SystemClock {
    fn now(&self) -> DateTime<Utc> {
        Utc::now()
    }
}
