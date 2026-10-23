use std::time::{Duration, Instant};

// iOS may stop reporting write readiness in the background. Space fallback
// submissions to avoid flushing a queued batch when the callback is missing.
const WRITE_INTERVAL: Duration = Duration::from_millis(30);

#[derive(Default)]
pub(super) struct WriteGate {
    last_write: Option<Instant>,
}

impl WriteGate {
    pub(super) fn deadline(&self, queued: Instant) -> Instant {
        // Count idle time toward pacing instead of delaying every new request.
        self.last_write.map_or(queued + WRITE_INTERVAL, |last| {
            (last + WRITE_INTERVAL).max(queued)
        })
    }

    pub(super) fn submitted(&mut self, now: Instant) {
        self.last_write = Some(now);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn first_write_has_a_bounded_wait() {
        let queued = Instant::now();
        assert_eq!(
            WriteGate::default().deadline(queued),
            queued + WRITE_INTERVAL
        );
    }

    #[test]
    fn queued_writes_remain_paced_after_each_submission() {
        let mut gate = WriteGate::default();
        let queued = Instant::now();
        let first = queued + WRITE_INTERVAL;
        gate.submitted(first);
        assert_eq!(gate.deadline(queued), first + WRITE_INTERVAL);
        let second = first + WRITE_INTERVAL;
        gate.submitted(second);
        assert_eq!(gate.deadline(queued), second + WRITE_INTERVAL);
    }

    #[test]
    fn idle_time_counts_toward_the_interval() {
        let mut gate = WriteGate::default();
        let start = Instant::now();
        gate.submitted(start);
        assert_eq!(
            gate.deadline(start + Duration::from_millis(20)),
            start + WRITE_INTERVAL
        );
        let later = start + Duration::from_millis(100);
        assert_eq!(gate.deadline(later), later);
    }
}
