use std::collections::BTreeMap;
use std::time::Duration;

#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct TimerId(u64);

impl TimerId {
    pub const IDLE_COMMIT: Self = Self(1);

    pub const fn new(value: u64) -> Self {
        Self(value)
    }
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
/// A deterministic timer queue using monotonic elapsed durations supplied by
/// the shell; it never reads a clock.
pub struct TimerWheel {
    ordered: BTreeMap<(Duration, TimerId), ()>,
    deadlines: BTreeMap<TimerId, Duration>,
}

impl TimerWheel {
    pub fn schedule(&mut self, id: TimerId, deadline: Duration) -> Option<Duration> {
        let previous = self.deadlines.insert(id, deadline);
        if let Some(previous_deadline) = previous {
            self.ordered.remove(&(previous_deadline, id));
        }
        self.ordered.insert((deadline, id), ());
        previous
    }

    pub fn cancel(&mut self, id: TimerId) -> bool {
        let Some(deadline) = self.deadlines.remove(&id) else {
            return false;
        };
        self.ordered.remove(&(deadline, id));
        true
    }

    pub fn next_deadline(&self) -> Option<Duration> {
        self.ordered
            .first_key_value()
            .map(|((deadline, _), _)| *deadline)
    }

    pub fn has_due_timer(&self, now: Duration) -> bool {
        self.next_deadline().is_some_and(|deadline| deadline <= now)
    }

    pub fn drain_due(&mut self, now: Duration) -> Vec<TimerId> {
        let due: Vec<_> = self
            .ordered
            .keys()
            .take_while(|(deadline, _)| *deadline <= now)
            .copied()
            .collect();

        for (deadline, id) in &due {
            self.ordered.remove(&(*deadline, *id));
            self.deadlines.remove(id);
        }

        due.into_iter().map(|(_, id)| id).collect()
    }
}

pub fn check_no_due_armed_timers(wheel: &TimerWheel, now: Duration) -> Result<(), String> {
    if wheel.has_due_timer(now) {
        Err("an armed timer is due at or before the update time".to_owned())
    } else {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use super::{TimerId, TimerWheel};

    #[test]
    fn replacing_timer_moves_its_deadline() {
        let mut wheel = TimerWheel::default();
        let timer = TimerId::new(1);
        wheel.schedule(timer, Duration::from_secs(2));

        assert_eq!(
            wheel.schedule(timer, Duration::from_secs(4)),
            Some(Duration::from_secs(2))
        );
        assert!(!wheel.has_due_timer(Duration::from_secs(3)));
        assert_eq!(wheel.drain_due(Duration::from_secs(4)), [timer]);
        assert_eq!(wheel.next_deadline(), None);
    }

    #[test]
    fn cancellation_and_equal_deadline_order_are_deterministic() {
        let mut wheel = TimerWheel::default();
        let first = TimerId::new(1);
        let second = TimerId::new(2);
        let deadline = Duration::from_millis(100);
        wheel.schedule(second, deadline);
        wheel.schedule(first, deadline);

        assert!(wheel.cancel(second));
        assert!(!wheel.cancel(second));
        assert_eq!(wheel.drain_due(deadline), [first]);
    }

    #[test]
    fn drains_every_due_timer_and_preserves_future_timers() {
        let mut wheel = TimerWheel::default();
        let earlier = TimerId::new(1);
        let same_deadline = TimerId::new(2);
        let future = TimerId::new(3);
        wheel.schedule(same_deadline, Duration::from_secs(2));
        wheel.schedule(earlier, Duration::from_secs(1));
        wheel.schedule(future, Duration::from_secs(3));

        assert_eq!(
            wheel.drain_due(Duration::from_secs(2)),
            [earlier, same_deadline]
        );
        assert_eq!(wheel.next_deadline(), Some(Duration::from_secs(3)));
    }
}
