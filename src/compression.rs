use crate::{Result, IlbmError};
use crate::bytes::BigEndian;

/// The ILBM format can compress image data, if it does it uses this very simple run length encoding
/// 
///   LOOP until produced the desired number of bytes
///       Read the next source byte into n
///       SELECT n FROM
///           [0..127]   => copy the next n+1 bytes literally
///           [-1..-127] => replicate the next byte -n+1 times
///           -128       => no operation
///           ENDCASE;
///       ENDLOOP;
///
pub fn unpacker(input: &[u8], byte_width: usize) -> Result<(&[u8], Vec<u8>)> {
    let mut data = input;
    let mut unpacked: Vec<u8> = Vec::with_capacity(byte_width);

    while unpacked.len() < byte_width {
        let n = data.get_i8()? as i16;       

        if n >= 0 {
            for _i in 0..(n+1) {
                unpacked.push(data.get_u8()?);
            }
        } else if n != -128 {
            let b = data.get_u8()?;

            for _i in 0..(-n + 1) {
                unpacked.push(b);
            }
        }
    }

    if unpacked.len() != byte_width {
        Err(IlbmError::InvalidData(format!("decompression unpacked too many bytes, expected {} but got {}", byte_width, unpacked.len())))
    } else {
        Ok((data, unpacked))
    }
}

#[cfg(test)]
mod tests {
    use super::unpacker;

    #[test]
    fn unpack_1() {
        let compressed = [0u8, 66u8];
        let expected = [66u8];

        let (remaining, unpacked) = unpacker(&compressed, 1).unwrap();

        assert_eq!(unpacked, expected);
        assert_eq!(remaining.len(), 0);
    }

    #[test]
    fn unpack_2() {
        let compressed = [0u8, 66u8, 67u8];
        let expected = [66u8];

        let (remaining, unpacked) = unpacker(&compressed, 1).unwrap();

        assert_eq!(unpacked, expected);
        assert_eq!(remaining.len(), 1);
    }

    #[test]
    fn unpack_3() {
        let compressed = [2u8, 66u8, 67u8, 68u8, 69u8];
        let expected = [66u8, 67u8, 68u8];

        let (remaining, unpacked) = unpacker(&compressed, 3).unwrap();

        assert_eq!(unpacked, expected);
        assert_eq!(remaining.len(), 1);
    }

    #[test]
    #[should_panic]
    fn unpack_4() {
        let compressed = [2u8, 66u8, 67u8]; // This is intentionally broken, not enough data
        let (_remaining, _unpacked) = unpacker(&compressed, 3).unwrap();
    }

    #[test]
    fn unpack_5() {
        let compressed = [255u8, 10u8];
        let expected = [10u8, 10u8];

        let (remaining, unpacked) = unpacker(&compressed, 2).unwrap();

        assert_eq!(unpacked, expected);
        assert_eq!(remaining.len(), 0);
    }

    #[test]
    fn unpack_6() {
        let compressed = [253u8, 10u8];
        let expected = [10u8, 10u8, 10u8, 10u8];

        let (remaining, unpacked) = unpacker(&compressed, 4).unwrap();

        assert_eq!(unpacked, expected);
        assert_eq!(remaining.len(), 0);
    }

    #[test]
    #[should_panic]
    fn unpack_7() {
        let compressed = [250u8, 10u8]; // Broken, will generate too much data
        let (_remaining, _unpacked) = unpacker(&compressed, 1).unwrap();
    }
}