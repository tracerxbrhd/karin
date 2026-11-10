// SPDX-License-Identifier: GPL-3.0-or-later

//! Bounded format screening for a completed private HTTP MP3 response.
//!
//! This checks tag boundaries and two complete consecutive Layer III frames. It
//! is not a decoder, a CRC check, or proof that every frame can be played. HTTP
//! policy, the complete-response length and the stored digest remain separate
//! admission and integrity checks.

use std::{fs::File, io::ErrorKind, os::unix::fs::FileExt};

use super::{CacheError, Result};

const MAX_LEADING_TAGS: usize = 4;

pub(super) fn validate(file: &File, length: u64) -> Result<()> {
    let metadata = file.metadata().map_err(|_| CacheError::Unavailable)?;
    if !metadata.is_file()
        || metadata.len() != length
        || length > super::MAX_AUDIO as u64
        || length < 8
    {
        return Err(CacheError::InvalidInput);
    }
    let offset = skip_tags(file, length)?;
    let first = frame(file, offset, length)?;
    let second_offset = offset
        .checked_add(first.length)
        .ok_or(CacheError::InvalidInput)?;
    let second = frame(file, second_offset, length)?;
    if first.version != second.version || first.sample_rate != second.sample_rate {
        return Err(CacheError::InvalidInput);
    }
    Ok(())
}

fn skip_tags(file: &File, length: u64) -> Result<u64> {
    let mut offset = 0u64;
    for _ in 0..MAX_LEADING_TAGS {
        let prefix: [u8; 3] = read(file, offset, length)?;
        if prefix != *b"ID3" {
            return Ok(offset);
        }
        let header: [u8; 10] = read(file, offset, length)?;
        let flags = header[5];
        let valid_flags = match header[3] {
            // ID3v2.2 whole-tag compression is unspecified and unsupported here.
            2 => flags & 0x7f == 0,
            3 => flags & 0x1f == 0,
            4 => flags & 0x0f == 0,
            _ => false,
        };
        if !valid_flags || header[4] == 0xff || header[6..10].iter().any(|v| v & 0x80 != 0) {
            return Err(CacheError::InvalidInput);
        }
        let tag_length = header[6..10]
            .iter()
            .fold(0u64, |size, byte| (size << 7) | u64::from(*byte));
        let footer_offset = offset
            .checked_add(10)
            .and_then(|value| value.checked_add(tag_length))
            .filter(|value| *value <= length)
            .ok_or(CacheError::InvalidInput)?;
        offset = footer_offset;
        if header[3] == 4 && flags & 0x10 != 0 {
            let footer: [u8; 10] = read(file, footer_offset, length)?;
            if footer[..3] != *b"3DI" || footer[3..] != header[3..] {
                return Err(CacheError::InvalidInput);
            }
            offset = offset.checked_add(10).ok_or(CacheError::InvalidInput)?;
        }
    }
    // A fifth leading tag is unsupported; never scan an unbounded prefix.
    if read::<3>(file, offset, length)? == *b"ID3" {
        return Err(CacheError::InvalidInput);
    }
    Ok(offset)
}

struct Frame {
    version: u8,
    sample_rate: u32,
    length: u64,
}

fn frame(file: &File, offset: u64, length: u64) -> Result<Frame> {
    let header = u32::from_be_bytes(read(file, offset, length)?);
    let version = ((header >> 19) & 3) as u8;
    let layer = (header >> 17) & 3;
    let bitrate_index = ((header >> 12) & 15) as usize;
    let rate_index = ((header >> 10) & 3) as usize;
    if header >> 21 != 0x7ff
        || version == 1
        || layer != 1
        || bitrate_index == 0
        || bitrate_index == 15
        || rate_index == 3
        || header & 3 == 2
    {
        return Err(CacheError::InvalidInput);
    }
    const MPEG1_BITRATES: [u32; 14] = [
        32, 40, 48, 56, 64, 80, 96, 112, 128, 160, 192, 224, 256, 320,
    ];
    const MPEG2_BITRATES: [u32; 14] = [8, 16, 24, 32, 40, 48, 56, 64, 80, 96, 112, 128, 144, 160];
    let bitrate = if version == 3 {
        MPEG1_BITRATES[bitrate_index - 1]
    } else {
        MPEG2_BITRATES[bitrate_index - 1]
    };
    let divisor = match version {
        3 => 1,
        2 => 2,
        0 => 4,
        _ => return Err(CacheError::InvalidInput),
    };
    let sample_rate = [44_100, 48_000, 32_000][rate_index] / divisor;
    let coefficient = if version == 3 { 144_000 } else { 72_000 };
    let padding = (header >> 9) & 1;
    let frame_length = u64::from(coefficient * bitrate / sample_rate + padding);
    if offset
        .checked_add(frame_length)
        .is_none_or(|end| end > length)
    {
        return Err(CacheError::InvalidInput);
    }
    Ok(Frame {
        version,
        sample_rate,
        length: frame_length,
    })
}

fn read<const N: usize>(file: &File, offset: u64, length: u64) -> Result<[u8; N]> {
    if offset.checked_add(N as u64).is_none_or(|end| end > length) {
        return Err(CacheError::InvalidInput);
    }
    let mut value = [0u8; N];
    file.read_exact_at(&mut value, offset).map_err(|error| {
        if error.kind() == ErrorKind::UnexpectedEof {
            CacheError::InvalidInput
        } else {
            CacheError::Unavailable
        }
    })?;
    Ok(value)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn validate_bytes(bytes: &[u8]) -> Result<()> {
        let directory = super::super::Sandbox::create(&std::env::temp_dir()).unwrap();
        let path = directory.path.join("original-structural-mp3-fixture");
        std::fs::write(&path, bytes).unwrap();
        let file = File::open(path).unwrap();
        validate(&file, bytes.len() as u64)
    }

    // Original generated structural fixtures, not a copied recording. Zero frame
    // payloads are enough for this signature check and are not an audio test.
    fn generated_frame(version: u8, bitrate: usize, rate: usize, padding: bool) -> Vec<u8> {
        let header = 0x7ffu32 << 21
            | u32::from(version) << 19
            | 1 << 17
            | 1 << 16
            | (bitrate as u32) << 12
            | (rate as u32) << 10
            | u32::from(padding) << 9;
        let kbps = if version == 3 {
            [
                0u32, 32, 40, 48, 56, 64, 80, 96, 112, 128, 160, 192, 224, 256, 320,
            ][bitrate]
        } else {
            [
                0u32, 8, 16, 24, 32, 40, 48, 56, 64, 80, 96, 112, 128, 144, 160,
            ][bitrate]
        };
        let divisor = if version == 3 {
            1
        } else if version == 2 {
            2
        } else {
            4
        };
        let sample_rate = [44_100u32, 48_000, 32_000][rate] / divisor;
        let coefficient = if version == 3 { 144_000 } else { 72_000 };
        let length = coefficient * kbps / sample_rate + u32::from(padding);
        let mut bytes = vec![0; length as usize];
        bytes[..4].copy_from_slice(&header.to_be_bytes());
        bytes
    }

    fn audio() -> Vec<u8> {
        [
            generated_frame(3, 9, 0, false),
            generated_frame(3, 9, 0, false),
        ]
        .concat()
    }

    fn tagged(version: u8, footer: bool, payload: &[u8], audio: &[u8]) -> Vec<u8> {
        let size = payload.len() as u32;
        let header = [
            b'I',
            b'D',
            b'3',
            version,
            0,
            if footer { 0x10 } else { 0 },
            ((size >> 21) & 0x7f) as u8,
            ((size >> 14) & 0x7f) as u8,
            ((size >> 7) & 0x7f) as u8,
            (size & 0x7f) as u8,
        ];
        let mut bytes = [header.as_slice(), payload].concat();
        if footer {
            bytes.extend_from_slice(b"3DI");
            bytes.extend_from_slice(&header[3..]);
        }
        bytes.extend_from_slice(audio);
        bytes
    }

    #[test]
    fn complete_consecutive_layer_three_frames_accept_vbr_padding_and_lower_rates() {
        for version in [3, 2, 0] {
            for rate in 0..3 {
                let bytes = [
                    generated_frame(version, 9, rate, true),
                    generated_frame(version, 12, rate, false),
                    b"TAGoriginal trailing metadata".to_vec(),
                ]
                .concat();
                assert_eq!(validate_bytes(&bytes), Ok(()));
            }
        }
    }

    #[test]
    fn skips_bounded_id3_tags_and_checks_version_four_footer() {
        for version in [2, 3, 4] {
            assert_eq!(
                validate_bytes(&tagged(version, false, &[0; 20], &audio())),
                Ok(())
            );
        }
        assert_eq!(validate_bytes(&tagged(4, true, &[0; 20], &audio())), Ok(()));
        let nested = tagged(3, false, &[], &tagged(4, true, &[0; 20], &audio()));
        assert_eq!(validate_bytes(&nested), Ok(()));
        let mut corrupt_footer = tagged(4, true, &[0; 20], &audio());
        corrupt_footer[30] = b'X';
        assert_eq!(
            validate_bytes(&corrupt_footer),
            Err(CacheError::InvalidInput)
        );
    }

    #[test]
    fn refuses_html_compressed_bodies_tag_only_and_truncated_frames() {
        let mut truncated = audio();
        truncated.pop();
        for bytes in [
            b"<!doctype html><html>an error labeled audio/mpeg</html>".to_vec(),
            [vec![0x1f, 0x8b, 8, 0], audio()].concat(),
            tagged(4, false, &[0; 20], &[]),
            tagged(3, false, &[0; 20], b"<html>not MPEG frames</html>"),
            generated_frame(3, 9, 0, false),
            truncated,
        ] {
            assert_eq!(validate_bytes(&bytes), Err(CacheError::InvalidInput));
        }
    }

    #[test]
    fn refuses_incompatible_and_unsupported_mpeg_headers() {
        for second in [
            generated_frame(2, 9, 0, false),
            generated_frame(3, 9, 1, false),
        ] {
            assert_eq!(
                validate_bytes(&[generated_frame(3, 9, 0, false), second].concat()),
                Err(CacheError::InvalidInput)
            );
        }
        for (byte, mask, bits) in [
            (1, 0x18, 0x08), // Reserved MPEG version.
            (1, 0x06, 0x04), // Layer II, not MP3.
            (2, 0xf0, 0),    // Unsupported free-format bitrate.
            (2, 0xf0, 0xf0), // Invalid bitrate.
            (2, 0x0c, 0x0c), // Reserved sample rate.
            (3, 0x03, 0x02), // Reserved emphasis.
        ] {
            let mut bytes = audio();
            bytes[byte] = (bytes[byte] & !mask) | bits;
            assert_eq!(validate_bytes(&bytes), Err(CacheError::InvalidInput));
        }
    }

    #[test]
    fn refuses_invalid_tag_bounds_flags_compression_and_unbounded_prefixes() {
        for (byte, value) in [
            (3, 5),
            (4, 0xff),
            (5, 1),
            (6, 0x80),
            (6, 0x7f), // Declared size exceeds the actual fixture.
        ] {
            let mut bytes = tagged(3, false, &[], &audio());
            bytes[byte] = value;
            assert_eq!(validate_bytes(&bytes), Err(CacheError::InvalidInput));
        }
        let mut compressed_tag = tagged(2, false, &[], &audio());
        compressed_tag[5] = 0x40;
        assert_eq!(
            validate_bytes(&compressed_tag),
            Err(CacheError::InvalidInput)
        );
        let mut bytes = audio();
        for _ in 0..=MAX_LEADING_TAGS {
            bytes = tagged(3, false, &[], &bytes);
        }
        assert_eq!(validate_bytes(&bytes), Err(CacheError::InvalidInput));
    }

    #[test]
    fn rejects_wrong_length_and_oversized_files_without_reading_the_body() {
        let directory = super::super::Sandbox::create(&std::env::temp_dir()).unwrap();
        let path = directory.path.join("bounded-mp3-fixture");
        std::fs::write(&path, audio()).unwrap();
        let file = File::options().read(true).write(true).open(path).unwrap();
        assert_eq!(validate(&file, 12), Err(CacheError::InvalidInput));
        let excessive = super::super::MAX_AUDIO as u64 + 1;
        file.set_len(excessive).unwrap();
        assert_eq!(validate(&file, excessive), Err(CacheError::InvalidInput));
    }
}
