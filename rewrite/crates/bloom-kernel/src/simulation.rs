use std::collections::{BTreeMap, VecDeque};
use std::convert::Infallible;
use std::fmt;

use crate::ports::{Clock, Fs, Rng, Watcher};

pub const REPLAY_FORMAT_VERSION: u16 = 1;
const REPLAY_HEADER: &str = "BLOOM-SIM-REPLAY 1";

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum FsOperation {
    Read,
    Write,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum SimFsError {
    NotFound(String),
    Injected(FsOperation),
}

#[derive(Default)]
pub struct SimFs {
    files: BTreeMap<String, Vec<u8>>,
    failures: VecDeque<FsOperation>,
}

impl SimFs {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn insert(&mut self, path: impl Into<String>, bytes: Vec<u8>) {
        self.files.insert(path.into(), bytes);
    }

    pub fn file(&self, path: &str) -> Option<&[u8]> {
        self.files.get(path).map(Vec::as_slice)
    }

    pub fn fail_next(&mut self, operation: FsOperation) {
        self.failures.push_back(operation);
    }

    fn take_failure(&mut self, operation: FsOperation) -> bool {
        if self.failures.front() == Some(&operation) {
            self.failures.pop_front();
            true
        } else {
            false
        }
    }
}

impl Fs for SimFs {
    type Path = str;
    type Bytes = Vec<u8>;
    type Error = SimFsError;

    fn read(&mut self, path: &Self::Path) -> Result<Self::Bytes, Self::Error> {
        if self.take_failure(FsOperation::Read) {
            return Err(SimFsError::Injected(FsOperation::Read));
        }
        self.files
            .get(path)
            .cloned()
            .ok_or_else(|| SimFsError::NotFound(path.to_owned()))
    }

    fn write(&mut self, path: &Self::Path, bytes: &[u8]) -> Result<(), Self::Error> {
        if self.take_failure(FsOperation::Write) {
            return Err(SimFsError::Injected(FsOperation::Write));
        }
        self.files.insert(path.to_owned(), bytes.to_vec());
        Ok(())
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SimClock {
    tick: u64,
}

impl SimClock {
    pub const fn new(starting_tick: u64) -> Self {
        Self {
            tick: starting_tick,
        }
    }

    pub const fn advance_by(&mut self, ticks: u64) -> Result<(), SimClockError> {
        match self.tick.checked_add(ticks) {
            Some(tick) => {
                self.tick = tick;
                Ok(())
            }
            None => Err(SimClockError::Overflow),
        }
    }
}

impl Clock for SimClock {
    type Instant = u64;

    fn now(&self) -> Self::Instant {
        self.tick
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SimClockError {
    Overflow,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum WatcherEventKind {
    Created,
    Changed,
    Removed,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct WatcherEvent {
    pub path: String,
    pub kind: WatcherEventKind,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SimWatcherError {
    Injected,
}

#[derive(Default)]
pub struct SimWatcher {
    events: VecDeque<WatcherEvent>,
    failures: usize,
}

impl SimWatcher {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn queue(&mut self, event: WatcherEvent) {
        self.events.push_back(event);
    }

    pub fn fail_next(&mut self) {
        self.failures = self.failures.saturating_add(1);
    }
}

impl Watcher for SimWatcher {
    type Event = WatcherEvent;
    type Error = SimWatcherError;

    fn try_next(&mut self) -> Result<Option<Self::Event>, Self::Error> {
        if self.failures > 0 {
            self.failures -= 1;
            return Err(SimWatcherError::Injected);
        }
        Ok(self.events.pop_front())
    }
}

pub struct SeededRng {
    state: u64,
}

impl SeededRng {
    pub const fn new(seed: u64) -> Self {
        Self { state: seed }
    }

    fn next_u64(&mut self) -> u64 {
        self.state = self.state.wrapping_add(0x9e37_79b9_7f4a_7c15);
        let mut value = self.state;
        value = (value ^ (value >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
        value = (value ^ (value >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
        value ^ (value >> 31)
    }
}

impl Rng for SeededRng {
    type Error = Infallible;

    fn fill_bytes(&mut self, bytes: &mut [u8]) -> Result<(), Self::Error> {
        for chunk in bytes.chunks_mut(8) {
            let next = self.next_u64().to_le_bytes();
            chunk.copy_from_slice(&next[..chunk.len()]);
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SimFile {
    pub path: String,
    pub bytes: Vec<u8>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ReplayStep {
    Read { path: String },
    Write { path: String, bytes: Vec<u8> },
    FailNextFsRead,
    FailNextFsWrite,
    AdvanceClock { ticks: u64 },
    QueueWatcherEvent { event: WatcherEvent },
    FailNextWatcherPoll,
    PollWatcher,
    FillRandom { length: usize },
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ReplayCase {
    pub seed: u64,
    pub starting_tick: u64,
    pub files: Vec<SimFile>,
    pub steps: Vec<ReplayStep>,
}

impl ReplayCase {
    pub fn parse(input: &str) -> Result<Self, ReplayFormatError> {
        let mut lines = input.lines().enumerate();
        let (header_line, header) = lines.next().ok_or(ReplayFormatError::MissingHeader)?;
        let header_fields: Vec<_> = header.split_whitespace().collect();
        if header_fields.len() != 2 || header_fields[0] != "BLOOM-SIM-REPLAY" {
            return Err(ReplayFormatError::InvalidRecord {
                line: header_line + 1,
                reason: "expected replay header",
            });
        }
        let version = parse_decimal::<u16>(header_fields[1], header_line + 1)?;
        if version != REPLAY_FORMAT_VERSION {
            return Err(ReplayFormatError::UnsupportedVersion(version));
        }

        let (seed_line, seed_record) = lines
            .next()
            .ok_or(ReplayFormatError::MissingRecord("seed"))?;
        let seed_fields: Vec<_> = seed_record.split_whitespace().collect();
        let seed = if seed_fields.len() == 2 && seed_fields[0] == "seed" {
            if seed_fields[1].len() != 16
                || !seed_fields[1]
                    .bytes()
                    .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
            {
                return Err(ReplayFormatError::InvalidRecord {
                    line: seed_line + 1,
                    reason: "seed must be 16 lowercase hexadecimal digits",
                });
            }
            u64::from_str_radix(seed_fields[1], 16).map_err(|_| {
                ReplayFormatError::InvalidRecord {
                    line: seed_line + 1,
                    reason: "invalid seed",
                }
            })?
        } else {
            return Err(ReplayFormatError::InvalidRecord {
                line: seed_line + 1,
                reason: "expected seed record",
            });
        };

        let (clock_line, clock_record) = lines
            .next()
            .ok_or(ReplayFormatError::MissingRecord("clock"))?;
        let clock_fields: Vec<_> = clock_record.split_whitespace().collect();
        if clock_fields.len() != 2 || clock_fields[0] != "clock" {
            return Err(ReplayFormatError::InvalidRecord {
                line: clock_line + 1,
                reason: "expected clock record",
            });
        }
        let starting_tick = parse_decimal(clock_fields[1], clock_line + 1)?;

        let mut files = Vec::new();
        let mut steps = Vec::new();
        let mut saw_step = false;
        for (line_index, record) in lines {
            let line = line_index + 1;
            let fields: Vec<_> = record.split_whitespace().collect();
            match fields.first().copied() {
                Some("file") if !saw_step && fields.len() == 3 => {
                    let path = decode_path(fields[1], line)?;
                    let bytes = decode_hex(fields[2], line)?;
                    files.push(SimFile { path, bytes });
                }
                Some("step") => {
                    saw_step = true;
                    steps.push(parse_step(&fields, line)?);
                }
                _ => {
                    return Err(ReplayFormatError::InvalidRecord {
                        line,
                        reason: "unexpected or malformed record",
                    });
                }
            }
        }

        let case = Self {
            seed,
            starting_tick,
            files,
            steps,
        };
        case.validate()?;
        Ok(case)
    }

    pub fn encode(&self) -> Result<String, ReplayFormatError> {
        self.validate()?;
        let mut output = format!(
            "{REPLAY_HEADER}\nseed {:016x}\nclock {}\n",
            self.seed, self.starting_tick
        );
        for file in &self.files {
            output.push_str(&format!(
                "file {} {}\n",
                encode_hex(file.path.as_bytes()),
                encode_hex(&file.bytes)
            ));
        }
        for step in &self.steps {
            match step {
                ReplayStep::Read { path } => {
                    output.push_str(&format!("step read {}\n", encode_hex(path.as_bytes())));
                }
                ReplayStep::Write { path, bytes } => {
                    output.push_str(&format!(
                        "step write {} {}\n",
                        encode_hex(path.as_bytes()),
                        encode_hex(bytes)
                    ));
                }
                ReplayStep::FailNextFsRead => output.push_str("step fail-fs-read\n"),
                ReplayStep::FailNextFsWrite => output.push_str("step fail-fs-write\n"),
                ReplayStep::AdvanceClock { ticks } => {
                    output.push_str(&format!("step advance {ticks}\n"));
                }
                ReplayStep::QueueWatcherEvent { event } => output.push_str(&format!(
                    "step queue-watch {} {}\n",
                    encode_watch_kind(event.kind),
                    encode_hex(event.path.as_bytes())
                )),
                ReplayStep::FailNextWatcherPoll => {
                    output.push_str("step fail-watcher\n");
                }
                ReplayStep::PollWatcher => output.push_str("step poll-watch\n"),
                ReplayStep::FillRandom { length } => {
                    output.push_str(&format!("step random {length}\n"));
                }
            }
        }
        Ok(output)
    }

    fn validate(&self) -> Result<(), ReplayFormatError> {
        let mut paths = std::collections::BTreeSet::new();
        for file in &self.files {
            if file.path.is_empty() {
                return Err(ReplayFormatError::InvalidCase("file path cannot be empty"));
            }
            if !paths.insert(file.path.as_str()) {
                return Err(ReplayFormatError::DuplicatePath(file.path.clone()));
            }
        }
        for step in &self.steps {
            let path = match step {
                ReplayStep::Read { path }
                | ReplayStep::Write { path, .. }
                | ReplayStep::QueueWatcherEvent {
                    event: WatcherEvent { path, .. },
                } => Some(path),
                _ => None,
            };
            if path.is_some_and(String::is_empty) {
                return Err(ReplayFormatError::InvalidCase("step path cannot be empty"));
            }
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ReplayFormatError {
    MissingHeader,
    MissingRecord(&'static str),
    UnsupportedVersion(u16),
    DuplicatePath(String),
    InvalidCase(&'static str),
    InvalidRecord { line: usize, reason: &'static str },
}

impl fmt::Display for ReplayFormatError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::MissingHeader => formatter.write_str("missing replay header"),
            Self::MissingRecord(record) => write!(formatter, "missing replay {record} record"),
            Self::UnsupportedVersion(version) => {
                write!(formatter, "unsupported replay format version {version}")
            }
            Self::DuplicatePath(path) => write!(formatter, "duplicate initial file path {path:?}"),
            Self::InvalidCase(reason) => write!(formatter, "invalid replay case: {reason}"),
            Self::InvalidRecord { line, reason } => {
                write!(formatter, "invalid replay record at line {line}: {reason}")
            }
        }
    }
}

impl std::error::Error for ReplayFormatError {}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum FailureCode {
    FsPathMissing,
    FsReadInjected,
    FsWriteInjected,
    WatcherPollInjected,
    ClockOverflow,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct FailureSignature {
    pub format_version: u16,
    pub step: u64,
    pub code: FailureCode,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum StepObservation {
    FsFailureArmed,
    FsRead(Result<Vec<u8>, SimFsError>),
    FsWrite(Result<(), SimFsError>),
    ClockAdvanced(Result<u64, SimClockError>),
    WatcherEventQueued,
    WatcherFailureArmed,
    WatcherPoll(Result<Option<WatcherEvent>, SimWatcherError>),
    RandomBytes(Vec<u8>),
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ReplayResult {
    pub observations: Vec<StepObservation>,
    pub failures: Vec<FailureSignature>,
    pub files: Vec<SimFile>,
    pub final_tick: u64,
}

pub fn run_replay(case: &ReplayCase) -> Result<ReplayResult, ReplayFormatError> {
    case.validate()?;
    let mut fs = SimFs::new();
    for file in &case.files {
        fs.insert(file.path.clone(), file.bytes.clone());
    }
    let mut clock = SimClock::new(case.starting_tick);
    let mut watcher = SimWatcher::new();
    let mut rng = SeededRng::new(case.seed);
    let mut observations = Vec::with_capacity(case.steps.len());
    let mut failures = Vec::new();

    for (index, step) in case.steps.iter().enumerate() {
        let step_number = index as u64;
        let observation = match step {
            ReplayStep::Read { path } => {
                let result = fs.read(path);
                if let Err(error) = &result {
                    failures.push(FailureSignature {
                        format_version: REPLAY_FORMAT_VERSION,
                        step: step_number,
                        code: match error {
                            SimFsError::NotFound(_) => FailureCode::FsPathMissing,
                            SimFsError::Injected(FsOperation::Read) => FailureCode::FsReadInjected,
                            SimFsError::Injected(FsOperation::Write) => {
                                FailureCode::FsWriteInjected
                            }
                        },
                    });
                }
                StepObservation::FsRead(result)
            }
            ReplayStep::Write { path, bytes } => {
                let result = fs.write(path, bytes);
                if let Err(error) = &result {
                    failures.push(FailureSignature {
                        format_version: REPLAY_FORMAT_VERSION,
                        step: step_number,
                        code: match error {
                            SimFsError::Injected(_) => FailureCode::FsWriteInjected,
                            SimFsError::NotFound(_) => {
                                unreachable!("write does not read or require an existing file")
                            }
                        },
                    });
                }
                StepObservation::FsWrite(result)
            }
            ReplayStep::FailNextFsRead => {
                fs.fail_next(FsOperation::Read);
                StepObservation::FsFailureArmed
            }
            ReplayStep::FailNextFsWrite => {
                fs.fail_next(FsOperation::Write);
                StepObservation::FsFailureArmed
            }
            ReplayStep::AdvanceClock { ticks } => {
                let result = clock.advance_by(*ticks).map(|()| clock.now());
                if result == Err(SimClockError::Overflow) {
                    failures.push(FailureSignature {
                        format_version: REPLAY_FORMAT_VERSION,
                        step: step_number,
                        code: FailureCode::ClockOverflow,
                    });
                }
                StepObservation::ClockAdvanced(result)
            }
            ReplayStep::QueueWatcherEvent { event } => {
                watcher.queue(event.clone());
                StepObservation::WatcherEventQueued
            }
            ReplayStep::FailNextWatcherPoll => {
                watcher.fail_next();
                StepObservation::WatcherFailureArmed
            }
            ReplayStep::PollWatcher => {
                let result = watcher.try_next();
                if result == Err(SimWatcherError::Injected) {
                    failures.push(FailureSignature {
                        format_version: REPLAY_FORMAT_VERSION,
                        step: step_number,
                        code: FailureCode::WatcherPollInjected,
                    });
                }
                StepObservation::WatcherPoll(result)
            }
            ReplayStep::FillRandom { length } => {
                let mut bytes = vec![0; *length];
                match rng.fill_bytes(&mut bytes) {
                    Ok(()) => StepObservation::RandomBytes(bytes),
                    Err(never) => match never {},
                }
            }
        };
        observations.push(observation);
    }

    Ok(ReplayResult {
        observations,
        failures,
        files: fs
            .files
            .into_iter()
            .map(|(path, bytes)| SimFile { path, bytes })
            .collect(),
        final_tick: clock.now(),
    })
}

fn parse_step(fields: &[&str], line: usize) -> Result<ReplayStep, ReplayFormatError> {
    let invalid = || ReplayFormatError::InvalidRecord {
        line,
        reason: "unknown or malformed step",
    };
    match fields.get(1).copied() {
        Some("read") if fields.len() == 3 => Ok(ReplayStep::Read {
            path: decode_path(fields[2], line)?,
        }),
        Some("write") if fields.len() == 4 => Ok(ReplayStep::Write {
            path: decode_path(fields[2], line)?,
            bytes: decode_hex(fields[3], line)?,
        }),
        Some("fail-fs-read") if fields.len() == 2 => Ok(ReplayStep::FailNextFsRead),
        Some("fail-fs-write") if fields.len() == 2 => Ok(ReplayStep::FailNextFsWrite),
        Some("advance") if fields.len() == 3 => Ok(ReplayStep::AdvanceClock {
            ticks: parse_decimal(fields[2], line)?,
        }),
        Some("queue-watch") if fields.len() == 4 => Ok(ReplayStep::QueueWatcherEvent {
            event: WatcherEvent {
                kind: parse_watch_kind(fields[2], line)?,
                path: decode_path(fields[3], line)?,
            },
        }),
        Some("fail-watcher") if fields.len() == 2 => Ok(ReplayStep::FailNextWatcherPoll),
        Some("poll-watch") if fields.len() == 2 => Ok(ReplayStep::PollWatcher),
        Some("random") if fields.len() == 3 => Ok(ReplayStep::FillRandom {
            length: parse_decimal(fields[2], line)?,
        }),
        _ => Err(invalid()),
    }
}

fn parse_watch_kind(value: &str, line: usize) -> Result<WatcherEventKind, ReplayFormatError> {
    match value {
        "created" => Ok(WatcherEventKind::Created),
        "changed" => Ok(WatcherEventKind::Changed),
        "removed" => Ok(WatcherEventKind::Removed),
        _ => Err(ReplayFormatError::InvalidRecord {
            line,
            reason: "unknown watcher event kind",
        }),
    }
}

fn encode_watch_kind(kind: WatcherEventKind) -> &'static str {
    match kind {
        WatcherEventKind::Created => "created",
        WatcherEventKind::Changed => "changed",
        WatcherEventKind::Removed => "removed",
    }
}

fn parse_decimal<T>(value: &str, line: usize) -> Result<T, ReplayFormatError>
where
    T: std::str::FromStr + fmt::Display,
{
    let parsed = value
        .parse::<T>()
        .map_err(|_| ReplayFormatError::InvalidRecord {
            line,
            reason: "invalid decimal number",
        })?;
    if parsed.to_string() != value {
        return Err(ReplayFormatError::InvalidRecord {
            line,
            reason: "decimal number is not canonical",
        });
    }
    Ok(parsed)
}

fn decode_path(value: &str, line: usize) -> Result<String, ReplayFormatError> {
    let bytes = decode_hex(value, line)?;
    let path = String::from_utf8(bytes).map_err(|_| ReplayFormatError::InvalidRecord {
        line,
        reason: "path is not valid UTF-8",
    })?;
    if path.is_empty() {
        return Err(ReplayFormatError::InvalidRecord {
            line,
            reason: "path cannot be empty",
        });
    }
    Ok(path)
}

fn decode_hex(value: &str, line: usize) -> Result<Vec<u8>, ReplayFormatError> {
    if value == "-" {
        return Ok(Vec::new());
    }
    if value.len() & 1 != 0
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    {
        return Err(ReplayFormatError::InvalidRecord {
            line,
            reason: "hex field must use pairs of lowercase hexadecimal digits",
        });
    }
    value
        .as_bytes()
        .chunks(2)
        .map(|pair| {
            let digits =
                std::str::from_utf8(pair).map_err(|_| ReplayFormatError::InvalidRecord {
                    line,
                    reason: "invalid hexadecimal field",
                })?;
            u8::from_str_radix(digits, 16).map_err(|_| ReplayFormatError::InvalidRecord {
                line,
                reason: "invalid hexadecimal field",
            })
        })
        .collect()
}

fn encode_hex(bytes: &[u8]) -> String {
    if bytes.is_empty() {
        return "-".to_owned();
    }
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut encoded = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        encoded.push(HEX[(byte >> 4) as usize] as char);
        encoded.push(HEX[(byte & 0x0f) as usize] as char);
    }
    encoded
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::invariants::CheckerRegistry;

    const EMPTY_KERNEL_SMOKE: &str =
        include_str!("../../../testdata/replays/empty-kernel-smoke.sim");

    #[test]
    fn versioned_fixture_round_trips_and_replays_identically() {
        let case = ReplayCase::parse(EMPTY_KERNEL_SMOKE).unwrap();
        let encoded = case.encode().unwrap();
        assert_eq!(ReplayCase::parse(&encoded).unwrap(), case);

        let first = run_replay(&case).unwrap();
        let second = run_replay(&ReplayCase::parse(&encoded).unwrap()).unwrap();
        assert_eq!(first, second);
        assert_eq!(first.final_tick, 5);
        assert_eq!(first.files[0].bytes, b"updated");
        assert_eq!(
            first.failures,
            vec![
                FailureSignature {
                    format_version: REPLAY_FORMAT_VERSION,
                    step: 2,
                    code: FailureCode::FsReadInjected,
                },
                FailureSignature {
                    format_version: REPLAY_FORMAT_VERSION,
                    step: 4,
                    code: FailureCode::FsWriteInjected,
                },
                FailureSignature {
                    format_version: REPLAY_FORMAT_VERSION,
                    step: 10,
                    code: FailureCode::WatcherPollInjected,
                },
            ]
        );
        assert_eq!(
            first.observations[4],
            StepObservation::FsWrite(Err(SimFsError::Injected(FsOperation::Write)))
        );
        assert_eq!(
            first.observations[11],
            StepObservation::WatcherPoll(Ok(Some(WatcherEvent {
                path: "notes/a.md".to_owned(),
                kind: WatcherEventKind::Changed,
            })))
        );
        assert_eq!(
            first.observations[12],
            StepObservation::WatcherPoll(Ok(Some(WatcherEvent {
                path: "notes/b.md".to_owned(),
                kind: WatcherEventKind::Created,
            })))
        );
        assert_eq!(
            first.observations[13],
            StepObservation::WatcherPoll(Ok(Some(WatcherEvent {
                path: "notes/a.md".to_owned(),
                kind: WatcherEventKind::Changed,
            })))
        );

        let empty_registry = CheckerRegistry::new();
        assert!(empty_registry.run_applicable(&[]).unwrap().is_empty());
    }

    #[test]
    fn watcher_duplicates_order_empty_polls_and_errors_are_distinct() {
        let event = WatcherEvent {
            path: "notes/a.md".to_owned(),
            kind: WatcherEventKind::Changed,
        };
        let reordered = WatcherEvent {
            path: "notes/b.md".to_owned(),
            kind: WatcherEventKind::Created,
        };
        let mut watcher = SimWatcher::new();
        watcher.queue(event.clone());
        watcher.queue(reordered.clone());
        watcher.queue(event.clone());
        watcher.fail_next();

        assert_eq!(watcher.try_next(), Err(SimWatcherError::Injected));
        assert_eq!(watcher.try_next(), Ok(Some(event.clone())));
        assert_eq!(watcher.try_next(), Ok(Some(reordered)));
        assert_eq!(watcher.try_next(), Ok(Some(event)));
        assert_eq!(watcher.try_next(), Ok(None));
    }

    #[test]
    fn seeded_rng_is_repeatable_and_fs_failures_are_explicit() {
        let mut first = SeededRng::new(77);
        let mut second = SeededRng::new(77);
        let mut a = [0; 17];
        let mut b = [0; 17];
        first.fill_bytes(&mut a).unwrap();
        second.fill_bytes(&mut b).unwrap();
        assert_eq!(a, b);

        let mut fs = SimFs::new();
        fs.insert("notes/a.md", b"seed".to_vec());
        fs.fail_next(FsOperation::Read);
        assert_eq!(
            fs.read("notes/a.md"),
            Err(SimFsError::Injected(FsOperation::Read))
        );
        assert_eq!(
            fs.read("notes/missing.md"),
            Err(SimFsError::NotFound("notes/missing.md".to_owned()))
        );
        assert_eq!(fs.read("notes/a.md").unwrap(), b"seed");

        let mut clock = SimClock::new(u64::MAX);
        assert_eq!(clock.advance_by(1), Err(SimClockError::Overflow));
        assert_eq!(clock.now(), u64::MAX);
    }

    #[test]
    fn unsupported_versions_and_malformed_records_are_rejected() {
        assert_eq!(
            ReplayCase::parse("BLOOM-SIM-REPLAY 1\n"),
            Err(ReplayFormatError::MissingRecord("seed"))
        );
        assert_eq!(
            ReplayCase::parse("BLOOM-SIM-REPLAY 2\nseed 0000000000000000\nclock 0\n"),
            Err(ReplayFormatError::UnsupportedVersion(2))
        );
        assert!(matches!(
            ReplayCase::parse("BLOOM-SIM-REPLAY 1\nseed 0000000000000000\nclock 0\nfile 6e 0\n"),
            Err(ReplayFormatError::InvalidRecord { line: 4, .. })
        ));
    }

    #[test]
    fn codec_preserves_record_delimiters_and_rejects_invalid_cases() {
        let case = ReplayCase {
            seed: 1,
            starting_tick: 2,
            files: vec![SimFile {
                path: "notes/\na.md".to_owned(),
                bytes: Vec::new(),
            }],
            steps: vec![ReplayStep::Write {
                path: "notes/\tb.md".to_owned(),
                bytes: Vec::new(),
            }],
        };
        assert_eq!(ReplayCase::parse(&case.encode().unwrap()).unwrap(), case);

        let invalid = ReplayCase {
            files: vec![SimFile {
                path: String::new(),
                bytes: Vec::new(),
            }],
            ..case
        };
        assert_eq!(
            invalid.encode(),
            Err(ReplayFormatError::InvalidCase("file path cannot be empty"))
        );
    }
}
