//! Bounded compiler boundary for canonical WSM FS record streams.
//!
//! CML does not own WSM envelope meaning.  It preserves canonical records as
//! opaque UTF-8 payloads, applies image bounds, and exposes deterministic
//! ordered bytes to a storage/backend adapter.

pub const MAX_RECORD_BYTES: usize = 64 * 1024;
pub const MAX_RECORDS: usize = 4096;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FsRecordStream {
    records: Vec<Vec<u8>>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FsRecordError {
    Empty,
    TooManyRecords(usize),
    RecordTooLarge { index: usize, actual: usize },
    InvalidUtf8 { index: usize },
}

impl FsRecordStream {
    /// Accept newline-delimited canonical records without interpreting them.
    pub fn parse(input: &[u8]) -> Result<Self, FsRecordError> {
        let records: Vec<&[u8]> = input
            .split(|byte| *byte == b'\n')
            .filter(|record| !record.is_empty())
            .collect();
        if records.is_empty() {
            return Err(FsRecordError::Empty);
        }
        if records.len() > MAX_RECORDS {
            return Err(FsRecordError::TooManyRecords(records.len()));
        }
        let mut owned = Vec::with_capacity(records.len());
        for (index, record) in records.into_iter().enumerate() {
            if record.len() > MAX_RECORD_BYTES {
                return Err(FsRecordError::RecordTooLarge {
                    index,
                    actual: record.len(),
                });
            }
            if std::str::from_utf8(record).is_err() {
                return Err(FsRecordError::InvalidUtf8 { index });
            }
            owned.push(record.to_vec());
        }
        Ok(Self { records: owned })
    }

    pub fn records(&self) -> &[Vec<u8>] {
        &self.records
    }

    /// Re-emit the exact ordered bytes, with one newline per record.
    pub fn to_bytes(&self) -> Vec<u8> {
        let capacity = self.records.iter().map(Vec::len).sum::<usize>() + self.records.len();
        let mut output = Vec::with_capacity(capacity);
        for record in &self.records {
            output.extend_from_slice(record);
            output.push(b'\n');
        }
        output
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn preserves_order_and_bytes() {
        let input = b"root\nobject\n";
        let stream = FsRecordStream::parse(input).unwrap();
        assert_eq!(stream.records(), &[b"root".to_vec(), b"object".to_vec()]);
        assert_eq!(stream.to_bytes(), input);
    }

    #[test]
    fn rejects_empty_invalid_utf8_and_oversized_records() {
        assert_eq!(FsRecordStream::parse(b"\n"), Err(FsRecordError::Empty));
        assert_eq!(
            FsRecordStream::parse(&[0xff]),
            Err(FsRecordError::InvalidUtf8 { index: 0 })
        );
        let oversized = vec![b'x'; MAX_RECORD_BYTES + 1];
        assert_eq!(
            FsRecordStream::parse(&oversized),
            Err(FsRecordError::RecordTooLarge {
                index: 0,
                actual: MAX_RECORD_BYTES + 1
            })
        );
    }
}
