//! Injected boundaries implemented by the imperative shell and test fakes.

/// Filesystem access owned by an adapter, never called by the pure kernel.
///
/// This is only an I/O boundary. Compare-and-swap, atomicity, and durability
/// protocols are defined by later milestones.
pub trait Fs {
    type Path: ?Sized;
    type Bytes;
    type Error;

    fn read(&mut self, path: &Self::Path) -> Result<Self::Bytes, Self::Error>;
    fn write(&mut self, path: &Self::Path, bytes: &[u8]) -> Result<(), Self::Error>;
}

/// A source of explicitly supplied, comparable time values.
pub trait Clock {
    type Instant: Copy + Ord;

    fn now(&self) -> Self::Instant;
}

/// A watcher adapter that distinguishes an empty poll from a failure.
pub trait Watcher {
    type Event;
    type Error;

    fn try_next(&mut self) -> Result<Option<Self::Event>, Self::Error>;
}

/// A byte source whose entropy failures must be handled explicitly.
pub trait Rng {
    type Error;

    fn fill_bytes(&mut self, bytes: &mut [u8]) -> Result<(), Self::Error>;
}

#[cfg(test)]
mod tests {
    use std::collections::VecDeque;

    use super::{Clock, Fs, Rng, Watcher};

    struct FakeFs {
        fail: bool,
    }

    impl Fs for FakeFs {
        type Path = str;
        type Bytes = Vec<u8>;
        type Error = &'static str;

        fn read(&mut self, _path: &Self::Path) -> Result<Self::Bytes, Self::Error> {
            if self.fail {
                Err("read failed")
            } else {
                Ok(vec![1, 2, 3])
            }
        }

        fn write(&mut self, _path: &Self::Path, _bytes: &[u8]) -> Result<(), Self::Error> {
            if self.fail {
                Err("write failed")
            } else {
                Ok(())
            }
        }
    }

    struct FixedClock(u64);

    impl Clock for FixedClock {
        type Instant = u64;

        fn now(&self) -> Self::Instant {
            self.0
        }
    }

    struct FakeWatcher {
        events: VecDeque<&'static str>,
        fail: bool,
    }

    impl Watcher for FakeWatcher {
        type Event = &'static str;
        type Error = &'static str;

        fn try_next(&mut self) -> Result<Option<Self::Event>, Self::Error> {
            if self.fail {
                Err("watch failed")
            } else {
                Ok(self.events.pop_front())
            }
        }
    }

    struct FixedRng {
        next: u8,
        fail: bool,
    }

    impl Rng for FixedRng {
        type Error = &'static str;

        fn fill_bytes(&mut self, bytes: &mut [u8]) -> Result<(), Self::Error> {
            if self.fail {
                return Err("entropy unavailable");
            }
            for byte in bytes {
                *byte = self.next;
                self.next = self.next.wrapping_add(1);
            }
            Ok(())
        }
    }

    #[test]
    fn ports_supply_deterministic_values_and_explicit_failures() {
        assert_eq!(FixedClock(42).now(), 42);

        let mut rng = FixedRng {
            next: 7,
            fail: false,
        };
        let mut bytes = [0; 3];
        rng.fill_bytes(&mut bytes).unwrap();
        assert_eq!(bytes, [7, 8, 9]);
        rng.fail = true;
        assert_eq!(rng.fill_bytes(&mut bytes), Err("entropy unavailable"));

        let mut fs = FakeFs { fail: false };
        assert_eq!(fs.read("note.md").unwrap(), [1, 2, 3]);
        fs.fail = true;
        assert_eq!(fs.read("note.md"), Err("read failed"));

        let mut watcher = FakeWatcher {
            events: VecDeque::from(["changed"]),
            fail: false,
        };
        assert_eq!(watcher.try_next().unwrap(), Some("changed"));
        assert_eq!(watcher.try_next().unwrap(), None);
        watcher.fail = true;
        assert_eq!(watcher.try_next(), Err("watch failed"));
    }
}
