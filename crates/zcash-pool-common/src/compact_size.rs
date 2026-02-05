//! Bitcoin/Zcash CompactSize integer encoding and decoding.
//!
//! CompactSize is a variable-length integer encoding used in Bitcoin and Zcash
//! for serializing counts and lengths in the wire protocol.

/// Error type for CompactSize decoding failures.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CompactSizeError {
    /// Not enough bytes to read the value.
    OutOfBounds,
}

impl std::fmt::Display for CompactSizeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::OutOfBounds => write!(f, "compact size out of bounds"),
        }
    }
}

impl std::error::Error for CompactSizeError {}

/// Read a CompactSize-encoded integer from `data` starting at `cursor`.
/// Advances `cursor` past the encoded value.
pub fn read_compact_size(data: &[u8], cursor: &mut usize) -> Result<u64, CompactSizeError> {
    if *cursor >= data.len() {
        return Err(CompactSizeError::OutOfBounds);
    }
    let prefix = data[*cursor];
    *cursor += 1;
    match prefix {
        n @ 0x00..=0xfc => Ok(n as u64),
        0xfd => {
            if *cursor + 2 > data.len() {
                return Err(CompactSizeError::OutOfBounds);
            }
            let val = u16::from_le_bytes([data[*cursor], data[*cursor + 1]]) as u64;
            *cursor += 2;
            Ok(val)
        }
        0xfe => {
            if *cursor + 4 > data.len() {
                return Err(CompactSizeError::OutOfBounds);
            }
            let val = u32::from_le_bytes([
                data[*cursor],
                data[*cursor + 1],
                data[*cursor + 2],
                data[*cursor + 3],
            ]) as u64;
            *cursor += 4;
            Ok(val)
        }
        0xff => {
            if *cursor + 8 > data.len() {
                return Err(CompactSizeError::OutOfBounds);
            }
            let val = u64::from_le_bytes([
                data[*cursor],
                data[*cursor + 1],
                data[*cursor + 2],
                data[*cursor + 3],
                data[*cursor + 4],
                data[*cursor + 5],
                data[*cursor + 6],
                data[*cursor + 7],
            ]);
            *cursor += 8;
            Ok(val)
        }
    }
}

/// Write a CompactSize-encoded integer to `out`.
pub fn write_compact_size(value: u64, out: &mut Vec<u8>) {
    if value < 0xfd {
        out.push(value as u8);
    } else if value <= 0xffff {
        out.push(0xfd);
        out.extend_from_slice(&(value as u16).to_le_bytes());
    } else if value <= 0xffff_ffff {
        out.push(0xfe);
        out.extend_from_slice(&(value as u32).to_le_bytes());
    } else {
        out.push(0xff);
        out.extend_from_slice(&value.to_le_bytes());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_write_read_roundtrip() {
        let test_values: &[u64] = &[0, 1, 0xfc, 0xfd, 0xffff, 0x10000, 0xffff_ffff, 0x1_0000_0000, 1344];
        for &val in test_values {
            let mut buf = Vec::new();
            write_compact_size(val, &mut buf);
            let mut cursor = 0;
            let decoded = read_compact_size(&buf, &mut cursor).unwrap();
            assert_eq!(val, decoded, "roundtrip failed for value {}", val);
            assert_eq!(cursor, buf.len(), "cursor not at end for value {}", val);
        }
    }

    #[test]
    fn test_known_encoding_1344() {
        // 1344 = 0x0540, encoded as 0xfd 0x40 0x05 (little-endian u16)
        let mut buf = Vec::new();
        write_compact_size(1344, &mut buf);
        assert_eq!(buf, vec![0xfd, 0x40, 0x05]);
    }

    #[test]
    fn test_single_byte_values() {
        let mut buf = Vec::new();
        write_compact_size(0, &mut buf);
        assert_eq!(buf, vec![0x00]);

        buf.clear();
        write_compact_size(252, &mut buf);
        assert_eq!(buf, vec![0xfc]);
    }

    #[test]
    fn test_read_out_of_bounds() {
        let mut cursor = 0;
        let result = read_compact_size(&[], &mut cursor);
        assert!(result.is_err());
    }

    #[test]
    fn test_read_truncated_u16() {
        let data = [0xfd, 0x40]; // missing second byte
        let mut cursor = 0;
        let result = read_compact_size(&data, &mut cursor);
        assert!(result.is_err());
    }
}
