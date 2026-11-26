//! Gzip for Create's schematic files, as read and written by `NbtIo`.

use flate2::{Compression, Crc, Decompress, FlushDecompress, Status, write::DeflateEncoder};
use std::io::{self, Write};

const MAGIC: [u8; 2] = [0x1f, 0x8b];

/// Why a gzip stream could not be read.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GzipError {
    /// Invalid or incomplete member framing, including an invalid header CRC.
    Header,
    /// An invalid or incomplete DEFLATE stream.
    Data,
    /// More decoded bytes than the caller allows.
    TooLarge,
    /// The trailer's CRC-32 or length does not match what was read.
    Check,
}

/// `bytes` as one gzip member at level 6, the OpenJDK `GZIPOutputStream` default.
pub fn encode(bytes: &[u8]) -> io::Result<Vec<u8>> {
    // Magic, deflate, no flags, no time, no extra flags, an unknown system.
    let header = vec![MAGIC[0], MAGIC[1], 8, 0, 0, 0, 0, 0, 0, 0xff];
    let mut encoder = DeflateEncoder::new(header, Compression::new(6));
    encoder.write_all(bytes)?;
    let mut out = encoder.finish()?;
    let mut crc = Crc::new();
    crc.update(bytes);
    out.extend_from_slice(&crc.sum().to_le_bytes());
    out.extend_from_slice(&crc.amount().to_le_bytes());
    Ok(out)
}

/// Decode at most `max` bytes total across all members. Ignore nonmember bytes after a valid
/// member, but require any suffix beginning with the full gzip magic to be a valid member.
pub fn decode(mut data: &[u8], max: usize) -> Result<Vec<u8>, GzipError> {
    let mut out = Vec::new();
    loop {
        data = member(data, max, &mut out)?;
        if !data.starts_with(&MAGIC) {
            return Ok(out);
        }
    }
}

/// Append one member to `out` and return the bytes after its trailer.
fn member<'a>(data: &'a [u8], max: usize, out: &mut Vec<u8>) -> Result<&'a [u8], GzipError> {
    let (head, mut body) = data.split_first_chunk::<10>().ok_or(GzipError::Header)?;
    if !head.starts_with(&MAGIC) || head[2] != 8 {
        return Err(GzipError::Header);
    }
    let flags = head[3];
    // Like GZIPInputStream.readHeader, skip FEXTRA, FNAME and FCOMMENT without a
    // separate name or comment length limit, and validate FHCRC if present.
    if flags & 4 != 0 {
        let (len, rest) = body.split_first_chunk::<2>().ok_or(GzipError::Header)?;
        body = rest
            .get(usize::from(u16::from_le_bytes(*len))..)
            .ok_or(GzipError::Header)?;
    }
    for bit in [8, 16] {
        if flags & bit != 0 {
            let end = body.iter().position(|&b| b == 0).ok_or(GzipError::Header)?;
            body = &body[end + 1..];
        }
    }
    if flags & 2 != 0 {
        let (expected, rest) = body.split_first_chunk::<2>().ok_or(GzipError::Header)?;
        let mut crc = Crc::new();
        crc.update(&data[..data.len() - body.len()]);
        if crc.sum() as u16 != u16::from_le_bytes(*expected) {
            return Err(GzipError::Header);
        }
        body = rest;
    }

    let start = out.len();
    let mut decoder = Decompress::new(false);
    let mut scratch = [0; 8192];
    let mut offset = 0;
    let trailer = loop {
        let before = decoder.total_out();
        let remaining = max - out.len();
        // One extra scratch byte distinguishes a stream that ends exactly at the cap.
        let len = scratch.len().min(remaining.saturating_add(1));
        let status = decoder
            .decompress(&body[offset..], &mut scratch[..len], FlushDecompress::None)
            .map_err(|_| GzipError::Data)?;
        let consumed = decoder.total_in() as usize;
        let written = (decoder.total_out() - before) as usize;
        if written > remaining {
            return Err(GzipError::TooLarge);
        }
        out.extend_from_slice(&scratch[..written]);
        if status == Status::StreamEnd {
            break &body[consumed..];
        }
        if consumed == offset && written == 0 {
            return Err(GzipError::Data);
        }
        offset = consumed;
    };

    let (sum, trailer) = trailer.split_first_chunk::<4>().ok_or(GzipError::Header)?;
    let (size, rest) = trailer.split_first_chunk::<4>().ok_or(GzipError::Header)?;
    let mut crc = Crc::new();
    crc.update(&out[start..]);
    if crc.sum() != u32::from_le_bytes(*sum) || crc.amount() != u32::from_le_bytes(*size) {
        return Err(GzipError::Check);
    }
    Ok(rest)
}

#[cfg(test)]
mod tests {
    use super::*;

    // The port's former encoder: one stored block and CRC-32 0xcbf43926.
    const STORED_MEMBER: &[u8] = b"\x1f\x8b\x08\x00\x00\x00\x00\x00\x00\xff\
                                  \x01\x09\x00\xf6\xff123456789\x26\x39\xf4\xcb\x09\x00\x00\x00";

    #[test]
    fn encodes_and_decodes_a_member() {
        for len in [0usize, 1, 100, 0xFFFF, 0x1_0000, 200_000] {
            let data: Vec<u8> = (0..len).map(|i| (i * 31 % 251) as u8).collect();
            let gz = encode(&data).expect("gzip encoding");
            assert_eq!(decode(&gz, len).unwrap(), data);
        }
    }

    #[test]
    fn reads_a_prior_stored_stream() {
        assert_eq!(decode(STORED_MEMBER, 9).unwrap(), b"123456789");
    }

    #[test]
    fn reads_long_optional_fields_and_checks_the_header_crc() {
        let mut gz = STORED_MEMBER[..10].to_vec();
        gz[3] = 4 | 8 | 16 | 2; // FEXTRA, FNAME, FCOMMENT, FHCRC
        gz.extend_from_slice(&[7, 0, b'X', b'Y', 3, 0, b'a', b'b', 0]);
        gz.extend(std::iter::repeat_n(b'n', 65_536));
        gz.push(0);
        gz.extend(std::iter::repeat_n(b'c', 65_537));
        gz.push(0);
        let mut crc = Crc::new();
        crc.update(&gz);
        let crc_offset = gz.len();
        gz.extend_from_slice(&(crc.sum() as u16).to_le_bytes());
        gz.extend_from_slice(&STORED_MEMBER[10..]);
        assert_eq!(decode(&gz, 9).unwrap(), b"123456789");

        gz[crc_offset] ^= 1;
        assert_eq!(decode(&gz, 9), Err(GzipError::Header));
    }

    #[test]
    fn reads_a_fixed_huffman_stream() {
        // Python's `gzip.compress(b"hello hello hello hello", 9, mtime=0)`: one fixed-Huffman
        // block with a match.
        let gz = [
            0x1f, 0x8b, 0x08, 0x00, 0x00, 0x00, 0x00, 0x00, 0x02, 0xff, 0xcb, 0x48, 0xcd, 0xc9,
            0xc9, 0x57, 0xc8, 0x40, 0x27, 0x01, 0xe3, 0x51, 0x3d, 0x8d, 0x17, 0x00, 0x00, 0x00,
        ];
        assert_eq!(decode(&gz, usize::MAX).unwrap(), b"hello hello hello hello");
    }

    #[test]
    fn reads_a_dynamic_huffman_stream() {
        // Python's `gzip.compress(text, 9, mtime=0)` of 300 random letters and spaces: one
        // dynamic-Huffman block.
        let text = "cbebhhhgdbhaggahedbfaaa agdga dhh dfddheag bcebf g deeh gahdggcf fbh bc gfhahaegcc dad  dg ffhe ag c  dgahf d ghfgfa  fhadc cb eabbahaedebcfebcce ceehfhhbaegfgdebe dgadagcach g d hd agfgaecdaebbeecgeca adhc agdfbdgdhbge hafgeacdfcfgdebg f h dbabccc def efffbedhc bfagbgccfbgb dbefe bhebaeaabgbaddgcbh";
        let gz = [
            0x1f, 0x8b, 0x08, 0x00, 0x00, 0x00, 0x00, 0x00, 0x02, 0xff, 0x1d, 0x8f, 0x41, 0x02,
            0x00, 0x51, 0x04, 0x42, 0xaf, 0xd2, 0xd5, 0x42, 0xe5, 0xfe, 0x27, 0x18, 0x7f, 0x16,
            0x16, 0xa8, 0x3c, 0x5d, 0xaa, 0xdd, 0xcd, 0xd4, 0x32, 0xe1, 0x6a, 0xca, 0x24, 0xc1,
            0x4c, 0x88, 0xd9, 0xc5, 0x78, 0x66, 0xc5, 0xa0, 0x5a, 0x65, 0x04, 0x23, 0x2d, 0x4e,
            0x3a, 0x49, 0x1b, 0xae, 0xbd, 0x0d, 0xe2, 0xe5, 0x52, 0xe9, 0xc6, 0x70, 0x80, 0x09,
            0xec, 0xd5, 0xe5, 0xa0, 0x5f, 0xc7, 0x35, 0x06, 0x59, 0xc7, 0x04, 0x4e, 0x3c, 0x8d,
            0x2e, 0x88, 0x55, 0xcf, 0x37, 0xaa, 0xf6, 0x55, 0x0b, 0x7d, 0xf1, 0xde, 0xad, 0x0b,
            0x73, 0x6e, 0xae, 0xe7, 0x1e, 0xa6, 0xd9, 0xfb, 0xae, 0x63, 0xe7, 0x52, 0x1d, 0xaa,
            0x87, 0xaa, 0x92, 0x3a, 0xea, 0x23, 0x9e, 0xed, 0x87, 0xed, 0x9a, 0xcc, 0x56, 0x84,
            0xa5, 0x23, 0xf6, 0xb8, 0xff, 0xa0, 0x23, 0xc2, 0xbd, 0x53, 0xbc, 0x33, 0x47, 0x29,
            0x43, 0xb6, 0x4b, 0xcf, 0x77, 0x4f, 0xa7, 0x0e, 0xde, 0x95, 0x3a, 0x89, 0x2c, 0xd4,
            0xea, 0x18, 0xc8, 0x9b, 0x70, 0x26, 0x5d, 0xfb, 0x01, 0xfe, 0x6d, 0x69, 0x0a, 0x2c,
            0x01, 0x00, 0x00,
        ];
        assert_eq!(decode(&gz, usize::MAX).unwrap(), text.as_bytes());
    }

    #[test]
    fn concatenated_members_share_one_decoded_budget() {
        let mut gz = encode(b"one").expect("gzip first member");
        gz.extend(encode(b"two").expect("gzip second member"));
        assert_eq!(decode(&gz, 3), Err(GzipError::TooLarge));
        assert_eq!(decode(&gz, 5), Err(GzipError::TooLarge));
        assert_eq!(decode(&gz, 6).unwrap(), b"onetwo");

        // A member with no output is still valid once the budget is exhausted.
        gz.extend(encode(b"").expect("gzip empty member"));
        assert_eq!(decode(&gz, 6).unwrap(), b"onetwo");
    }

    #[test]
    fn refuses_distance_before_start_of_member() {
        let mut incomplete_member = STORED_MEMBER[..10].to_vec();
        // RFC 1951 §3.2.3 forbids a distance before the start of the output.
        // Leave off the trailer: the raw decoder must reject the match first.
        incomplete_member.extend_from_slice(&[0x03, 0x02, 0x00]);
        assert_eq!(decode(&incomplete_member, usize::MAX), Err(GzipError::Data));

        let mut gz = encode(b"one").expect("gzip first member");
        gz.extend_from_slice(&incomplete_member);
        assert_eq!(decode(&gz, usize::MAX), Err(GzipError::Data));
    }

    #[test]
    fn ignores_nonmember_suffix_only_after_a_valid_member() {
        assert_eq!(decode(b"plain", 10), Err(GzipError::Header));
        let mut gz = encode(b"one").expect("gzip encoding");
        let end = gz.len();
        gz.extend_from_slice(b"plain");
        assert_eq!(decode(&gz, 10).unwrap(), b"one");

        gz.truncate(end);
        gz.push(MAGIC[0]);
        assert_eq!(decode(&gz, 10).unwrap(), b"one");
        gz.push(MAGIC[1]);
        assert_eq!(decode(&gz, 10), Err(GzipError::Header));
    }

    #[test]
    fn refuses_truncation_in_the_header_body_or_trailer() {
        for end in [9, 10, 16, STORED_MEMBER.len() - 1] {
            assert!(decode(&STORED_MEMBER[..end], 9).is_err());
        }
    }

    #[test]
    fn refuses_invalid_crc_or_length_even_at_the_decoded_limit() {
        let data = [7; 100];
        let gz = encode(&data).expect("gzip encoding");
        for offset in [8, 4] {
            let mut bad = gz.clone();
            bad[gz.len() - offset] ^= 1;
            assert_eq!(decode(&bad, data.len()), Err(GzipError::Check));
        }
    }
}
