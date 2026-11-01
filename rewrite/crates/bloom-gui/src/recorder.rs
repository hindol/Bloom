use std::collections::VecDeque;

pub const MAX_RECORDS: usize = 512;
pub const MAX_BYTES: usize = 16 * 1024;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum EventKind {
    InputAccepted,
    TimerFired,
    SaveCompleted,
    RecoveryCompleted,
    EffectCompleted,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum EffectOutcome {
    Success,
    Failure,
    Conflict,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct MetadataRecord {
    pub kind: EventKind,
    pub operation_id: u64,
    pub version: u64,
    pub elapsed_micros: u64,
    pub outcome: EffectOutcome,
}

const RECORD_BYTES: usize = std::mem::size_of::<MetadataRecord>();

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct RecorderLimits {
    pub max_records: usize,
    pub max_bytes: usize,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RecorderError {
    ZeroRecordLimit,
    RecordLimitExceeded { maximum: usize },
    ByteLimitTooSmall { minimum: usize },
    ByteLimitExceeded { maximum: usize },
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct RecordResult {
    pub evicted_records: usize,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct RecorderStats {
    pub retained_records: usize,
    pub retained_bytes: usize,
    pub evicted_records: u64,
}

#[derive(Debug)]
pub struct MetadataRecorder {
    limits: RecorderLimits,
    records: VecDeque<MetadataRecord>,
    evicted_records: u64,
}

impl MetadataRecorder {
    pub fn new() -> Self {
        Self::with_limits(RecorderLimits {
            max_records: MAX_RECORDS,
            max_bytes: MAX_BYTES,
        })
        .expect("default recorder limits are valid")
    }

    pub fn with_limits(limits: RecorderLimits) -> Result<Self, RecorderError> {
        if limits.max_records == 0 {
            return Err(RecorderError::ZeroRecordLimit);
        }
        if limits.max_records > MAX_RECORDS {
            return Err(RecorderError::RecordLimitExceeded {
                maximum: MAX_RECORDS,
            });
        }
        if limits.max_bytes < RECORD_BYTES {
            return Err(RecorderError::ByteLimitTooSmall {
                minimum: RECORD_BYTES,
            });
        }
        if limits.max_bytes > MAX_BYTES {
            return Err(RecorderError::ByteLimitExceeded { maximum: MAX_BYTES });
        }

        Ok(Self {
            limits,
            records: VecDeque::new(),
            evicted_records: 0,
        })
    }

    pub fn record(&mut self, record: MetadataRecord) -> RecordResult {
        let mut evicted = 0;
        while self.records.len() >= self.limits.max_records
            || self.retained_bytes() + RECORD_BYTES > self.limits.max_bytes
        {
            self.records.pop_front();
            evicted += 1;
            self.evicted_records = self.evicted_records.saturating_add(1);
        }
        self.records.push_back(record);
        RecordResult {
            evicted_records: evicted,
        }
    }

    pub fn records(&self) -> impl Iterator<Item = &MetadataRecord> {
        self.records.iter()
    }

    pub fn stats(&self) -> RecorderStats {
        RecorderStats {
            retained_records: self.records.len(),
            retained_bytes: self.retained_bytes(),
            evicted_records: self.evicted_records,
        }
    }

    fn retained_bytes(&self) -> usize {
        self.records.len() * RECORD_BYTES
    }
}

impl Default for MetadataRecorder {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::{
        EffectOutcome, EventKind, MetadataRecord, MetadataRecorder, RecorderError, RecorderLimits,
        MAX_BYTES, MAX_RECORDS,
    };

    fn record(operation_id: u64) -> MetadataRecord {
        MetadataRecord {
            kind: EventKind::EffectCompleted,
            operation_id,
            version: operation_id + 10,
            elapsed_micros: 42,
            outcome: EffectOutcome::Success,
        }
    }

    #[test]
    fn recorder_evicts_oldest_records_at_the_event_limit() {
        let mut recorder = MetadataRecorder::new();

        for operation_id in 0..(MAX_RECORDS as u64 + 2) {
            recorder.record(record(operation_id));
        }

        let records = recorder.records().copied().collect::<Vec<_>>();
        let stats = recorder.stats();
        assert_eq!(records.len(), MAX_RECORDS);
        assert_eq!(records[0].operation_id, 2);
        assert_eq!(stats.retained_records, MAX_RECORDS);
        assert!(stats.retained_bytes <= MAX_BYTES);
        assert_eq!(stats.evicted_records, 2);
    }

    #[test]
    fn byte_limit_is_enforced_independently_of_the_event_limit() {
        let record_bytes = std::mem::size_of::<MetadataRecord>();
        let mut recorder = MetadataRecorder::with_limits(RecorderLimits {
            max_records: 10,
            max_bytes: record_bytes * 2,
        })
        .unwrap();

        for operation_id in 0..4 {
            recorder.record(record(operation_id));
        }

        assert_eq!(recorder.records().count(), 2);
        assert_eq!(recorder.stats().retained_bytes, record_bytes * 2);
        assert_eq!(recorder.stats().evicted_records, 2);
    }

    #[test]
    fn recorder_rejects_limits_that_cannot_hold_one_record() {
        assert_eq!(
            MetadataRecorder::with_limits(RecorderLimits {
                max_records: 0,
                max_bytes: std::mem::size_of::<MetadataRecord>() * 2,
            })
            .err(),
            Some(RecorderError::ZeroRecordLimit)
        );
        assert_eq!(
            MetadataRecorder::with_limits(RecorderLimits {
                max_records: MAX_RECORDS + 1,
                max_bytes: MAX_BYTES,
            })
            .err(),
            Some(RecorderError::RecordLimitExceeded {
                maximum: MAX_RECORDS,
            })
        );
        assert_eq!(
            MetadataRecorder::with_limits(RecorderLimits {
                max_records: 1,
                max_bytes: MAX_BYTES + 1,
            })
            .err(),
            Some(RecorderError::ByteLimitExceeded { maximum: MAX_BYTES })
        );
        assert_eq!(
            MetadataRecorder::with_limits(RecorderLimits {
                max_records: 1,
                max_bytes: std::mem::size_of::<MetadataRecord>() - 1,
            })
            .err(),
            Some(RecorderError::ByteLimitTooSmall {
                minimum: std::mem::size_of::<MetadataRecord>(),
            })
        );
    }

    #[test]
    fn metadata_schema_has_no_content_bearing_fields() {
        let record = record(7);
        let debug = format!("{record:?}");

        assert!(!debug.contains("note text"));
        assert!(!debug.contains("private.md"));
        assert!(std::mem::size_of::<MetadataRecord>() <= MAX_BYTES / MAX_RECORDS);
    }
}
