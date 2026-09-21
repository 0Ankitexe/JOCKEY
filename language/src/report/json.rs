//! Serialize before touching output; the buffer never exceeds the byte budget.
use super::ReportError;
use crate::semantic::limits::MAX_REPORT_BYTES;
use serde::Serialize;
use std::io::{self, Write};

struct Buffer {
    bytes: Vec<u8>,
    limit: usize,
    overflowed: bool,
}
fn next_length(current: usize, additional: usize, limit: usize) -> Result<usize, ReportError> {
    current
        .checked_add(additional)
        .filter(|&n| n <= limit)
        .ok_or(ReportError::ResourceLimit)
}
impl Write for Buffer {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        if next_length(self.bytes.len(), bytes.len(), self.limit).is_err() {
            self.overflowed = true;
            return Err(io::ErrorKind::OutOfMemory.into());
        }
        self.bytes.extend_from_slice(bytes);
        Ok(bytes.len())
    }
    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}
pub(super) fn serialize(value: &impl Serialize) -> Result<Vec<u8>, ReportError> {
    serialize_with_limit(value, MAX_REPORT_BYTES)
}
fn serialize_with_limit(value: &impl Serialize, limit: usize) -> Result<Vec<u8>, ReportError> {
    let mut output = Buffer {
        bytes: Vec::new(),
        limit,
        overflowed: false,
    };
    let result = serde_json::to_writer_pretty(&mut output, value);
    if let Err(_error) = result {
        return Err(if output.overflowed {
            ReportError::ResourceLimit
        } else {
            ReportError::Serialization
        });
    }
    output
        .write_all(b"\n")
        .map_err(|_| ReportError::ResourceLimit)?;
    Ok(output.bytes)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn exact_eight_mib_boundary_includes_final_lf_and_rejects_the_next_byte() {
        assert_eq!(
            next_length(usize::MAX, 1, usize::MAX),
            Err(ReportError::ResourceLimit)
        );
        assert_eq!(
            next_length(MAX_REPORT_BYTES - 1, 1, MAX_REPORT_BYTES),
            Ok(MAX_REPORT_BYTES)
        );
        let text = "x".repeat(MAX_REPORT_BYTES - 3);
        assert_eq!(serialize(&text).unwrap().len(), MAX_REPORT_BYTES);
        assert_eq!(serialize(&(text + "x")), Err(ReportError::ResourceLimit));
        let mut b = Buffer {
            bytes: vec![0],
            limit: 1,
            overflowed: false,
        };
        assert!(b.write_all(&[0]).is_err());
        assert_eq!(b.bytes, vec![0]);
        assert!(b.overflowed);
    }
    #[test]
    fn injected_serializer_failure_discards_the_entire_buffer() {
        struct Broken;
        impl Serialize for Broken {
            fn serialize<S: serde::Serializer>(&self, _: S) -> Result<S::Ok, S::Error> {
                Err(serde::ser::Error::custom("must not escape"))
            }
        }
        assert_eq!(serialize(&Broken), Err(ReportError::Serialization));
    }
}
