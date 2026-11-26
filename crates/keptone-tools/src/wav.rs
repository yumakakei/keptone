// SPDX-FileCopyrightText: 2026 Yuma Kakei <yumasansansan@gmail.com>
// SPDX-License-Identifier: GPL-3.0-or-later
//
// This file is part of Keptone. The SPDX lines name its copyright holder and its
// license, in the machine-readable form of the REUSE specification: the GNU
// General Public License, version 3 or any later version
// (LICENSES/GPL-3.0-or-later.txt).

//! WAV files of IEEE floating-point samples, as the offline renderer writes
//! them (docs/DESIGN.md §11.5): a `RIFF` file of the `WAVE` form holding a
//! `fmt ` chunk of 18 bytes for format tag 3 with no extension, a `fact` chunk
//! that gives the count of frames, as the format asks of every format but PCM,
//! and the `data` chunk, every number little-endian and nothing else, so that
//! the same samples give the same bytes on every system.

/// The width of a file's samples.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Format {
    /// binary64, as the engine computes them.
    F64,
    /// binary32, each binary64 rounded to the nearest, ties to even, and one
    /// beyond binary32's largest number taken at it with its sign.
    F32,
}

impl Format {
    /// Its sample's width in bytes.
    const fn width(self) -> u16 {
        match self {
            Self::F64 => 8,
            Self::F32 => 4,
        }
    }
}

/// Why samples make no WAV file.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Error {
    /// No channel, or samples that do not make whole frames of the channels.
    Channels,
    /// Chunks that would not fit RIFF's 32-bit sizes, or a byte rate beyond
    /// them.
    Size,
}

/// The WAV format tag of IEEE floating-point samples.
const IEEE_FLOAT: u16 = 3;

/// The bytes of a WAV file of samples, interleaved frame by frame, in a count
/// of channels at a rate in samples a second, of a width.
///
/// # Errors
///
/// [`Error::Channels`] where there is no channel or the samples do not make
/// whole frames, and [`Error::Size`] where the file's chunks, or its byte
/// rate, would not fit RIFF's 32-bit sizes.
pub fn write(samples: &[f64], channels: u16, rate: u32, format: Format) -> Result<Vec<u8>, Error> {
    let count = usize::from(channels);
    if count == 0 || !samples.len().is_multiple_of(count) {
        return Err(Error::Channels);
    }
    let width = format.width();
    let frames = u32::try_from(samples.len() / count).map_err(|_| Error::Size)?;
    let block = channels.checked_mul(width).ok_or(Error::Size)?;
    let rate_bytes = rate.checked_mul(u32::from(block)).ok_or(Error::Size)?;
    let data = frames.checked_mul(u32::from(block)).ok_or(Error::Size)?;
    // The form's name, then the fmt chunk's header and 18 bytes, the fact
    // chunk's header and 4, and the data chunk's header.
    let riff = data.checked_add(4 + 8 + 18 + 8 + 4 + 8).ok_or(Error::Size)?;
    let total = usize::try_from(riff)
        .map_err(|_| Error::Size)?
        .checked_add(8)
        .ok_or(Error::Size)?;
    let mut bytes = Vec::with_capacity(total);
    bytes.extend_from_slice(b"RIFF");
    bytes.extend_from_slice(&riff.to_le_bytes());
    bytes.extend_from_slice(b"WAVE");
    bytes.extend_from_slice(b"fmt ");
    bytes.extend_from_slice(&18_u32.to_le_bytes());
    bytes.extend_from_slice(&IEEE_FLOAT.to_le_bytes());
    bytes.extend_from_slice(&channels.to_le_bytes());
    bytes.extend_from_slice(&rate.to_le_bytes());
    bytes.extend_from_slice(&rate_bytes.to_le_bytes());
    bytes.extend_from_slice(&block.to_le_bytes());
    bytes.extend_from_slice(&(8 * width).to_le_bytes());
    bytes.extend_from_slice(&0_u16.to_le_bytes());
    bytes.extend_from_slice(b"fact");
    bytes.extend_from_slice(&4_u32.to_le_bytes());
    bytes.extend_from_slice(&frames.to_le_bytes());
    bytes.extend_from_slice(b"data");
    bytes.extend_from_slice(&data.to_le_bytes());
    for &sample in samples {
        match format {
            Format::F64 => bytes.extend_from_slice(&sample.to_le_bytes()),
            Format::F32 => bytes.extend_from_slice(&narrowed(sample).to_le_bytes()),
        }
    }
    Ok(bytes)
}

/// A binary64 rounded to the nearest binary32, ties to even, one beyond
/// binary32's largest number taken at it with its sign first, so that no
/// sample becomes an infinity.
#[expect(
    clippy::as_conversions,
    clippy::cast_possible_truncation,
    reason = "Rust's conversion of a binary64 to binary32 rounds to the nearest, ties to even, and no From gives it"
)]
fn narrowed(sample: f64) -> f32 {
    let largest = f64::from(f32::MAX);
    sample.clamp(-largest, largest) as f32
}
