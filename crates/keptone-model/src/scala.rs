// SPDX-FileCopyrightText: 2026 Yuma Kakei <yumasansansan@gmail.com>
// SPDX-License-Identifier: GPL-3.0-or-later
//
// This file is part of Keptone. The SPDX lines name its copyright holder and its
// license, in the machine-readable form of the REUSE specification: the GNU
// General Public License, version 3 or any later version
// (LICENSES/GPL-3.0-or-later.txt).

//! Scala's tuning files, read by hand: a scale, `.scl` \[SCALA-SCL\], and a
//! keyboard mapping, `.kbm` \[SCALA-KBM\], which together give the tuning of
//! docs/MATH.md §2.1.
//!
//! The two formats are written for people, and their pages leave some things
//! open. What this reader does there, docs/DESIGN.md §11.4 says too:
//!
//! - A file is lines of bytes. A line ends at a line feed, a carriage return or
//!   the two together, so that files of every system read alike.
//! - A line whose first byte is `!` is a comment, and is passed over.
//! - A scale's description is its first line that is not a comment, whole, and
//!   may be empty. Its bytes are taken as UTF-8 where they are UTF-8, and as ISO
//!   8859-1 where they are not, as the scale archive keeps its files.
//! - After the description, and throughout a mapping, a line of nothing but
//!   spaces and tabs is passed over as a comment is.
//! - A value is the first run of characters on its line that are not spaces or
//!   tabs; what follows it on the line is passed over, as the format says of a
//!   pitch. A pitch holding a period is in cents, `-?[0-9]+\.[0-9]*`; any other
//!   is a ratio, `[0-9]+(/[0-9]+)?`, neither part 0, each up to 2⁶⁴ − 1, and a
//!   ratio alone is over 1. Integers are `-?[0-9]+`, and a frequency
//!   `[0-9]+(\.[0-9]*)?`, above 0.
//! - A mapping's four keys are MIDI's, 0 to 127. Its entries are degrees of the
//!   scale, any integer, or `x` for a key left unmapped; keys of the pattern
//!   after its last entry are unmapped, and the reference key may not be, as
//!   the format says.
//! - What follows a scale's last pitch, or the last entry that a mapping's
//!   size allows, is passed over.
//!
//! The parsers take any bytes and give a value or an error that names its
//! line, never a panic. They allocate as they read, never because a count in
//! the file says so, so that what they allocate is in proportion to the file.

use std::fmt;
use std::num::NonZeroU64;

/// A pitch of a scale above its 1/1, which the files leave out, as a `.scl`
/// file writes it.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Pitch {
    /// In cents.
    Cents(Cents),
    /// As a ratio.
    Ratio(Ratio),
}

/// A pitch in cents: a finite binary64, below 0 too.
#[derive(Debug, Clone, Copy, PartialEq, PartialOrd)]
pub struct Cents(f64);

impl Cents {
    /// The cents, if they are finite.
    #[must_use]
    pub fn new(cents: f64) -> Option<Self> {
        cents.is_finite().then_some(Self(cents))
    }

    /// The cents.
    #[must_use]
    pub const fn get(self) -> f64 {
        self.0
    }
}

/// A pitch as a ratio of two integers above 0, as written: 10/20 is kept as
/// it is, not reduced.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Ratio {
    /// The numerator.
    numerator: NonZeroU64,
    /// The denominator.
    denominator: NonZeroU64,
}

impl Ratio {
    /// The ratio of two integers above 0.
    #[must_use]
    pub const fn new(numerator: NonZeroU64, denominator: NonZeroU64) -> Self {
        Self { numerator, denominator }
    }

    /// The numerator.
    #[must_use]
    pub const fn numerator(self) -> NonZeroU64 {
        self.numerator
    }

    /// The denominator.
    #[must_use]
    pub const fn denominator(self) -> NonZeroU64 {
        self.denominator
    }
}

/// A scale, as a `.scl` file gives it.
#[derive(Debug, Clone, PartialEq)]
pub struct Scale {
    /// The description.
    pub description: String,
    /// The pitches above 1/1, in the order of the file. The last is the formal
    /// octave: the interval after which the scale repeats.
    pub pitches: Vec<Pitch>,
}

/// A MIDI key, 0 to 127.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Key(u8);

impl Key {
    /// The key of a number, if it is below 128.
    #[must_use]
    pub fn new(key: u8) -> Option<Self> {
        (key < 128).then_some(Self(key))
    }

    /// Its number.
    #[must_use]
    pub const fn get(self) -> u8 {
        self.0
    }
}

/// A frequency in hertz: a finite binary64 above 0.
#[derive(Debug, Clone, Copy, PartialEq, PartialOrd)]
pub struct Frequency(f64);

impl Frequency {
    /// The frequency, if it is finite and above 0.
    #[must_use]
    pub fn new(hertz: f64) -> Option<Self> {
        (hertz.is_finite() && hertz > 0.0).then_some(Self(hertz))
    }

    /// The frequency in hertz.
    #[must_use]
    pub const fn get(self) -> f64 {
        self.0
    }
}

/// A keyboard mapping, as a `.kbm` file gives it: which degree of the scale
/// each key plays, and the frequency of one key.
#[derive(Debug, Clone, PartialEq)]
pub struct Mapping {
    /// How many keys the pattern takes before it repeats a formal octave
    /// higher; 0 for a linear mapping, each key the next degree of the scale.
    size: usize,
    /// The first key retuned.
    first: Key,
    /// The last key retuned.
    last: Key,
    /// The key that the pattern's first entry is mapped to.
    middle: Key,
    /// The key whose frequency is given.
    reference: Key,
    /// That key's frequency.
    frequency: Frequency,
    /// The degree of the scale that is the formal octave; 0 for the last.
    octave: i64,
    /// The degree each key of the pattern plays, from the middle key up, or
    /// none for a key left unmapped; no more than the size.
    degrees: Vec<Option<i64>>,
}

impl Mapping {
    /// How many keys the pattern takes before it repeats a formal octave
    /// higher; 0 for a linear mapping, each key the next degree of the scale.
    #[must_use]
    pub const fn size(&self) -> usize {
        self.size
    }

    /// The first key retuned.
    #[must_use]
    pub const fn first(&self) -> Key {
        self.first
    }

    /// The last key retuned. A last key below the first retunes none.
    #[must_use]
    pub const fn last(&self) -> Key {
        self.last
    }

    /// The key that the pattern's first entry is mapped to.
    #[must_use]
    pub const fn middle(&self) -> Key {
        self.middle
    }

    /// The key whose frequency is given, which the mapping does not leave
    /// unmapped.
    #[must_use]
    pub const fn reference(&self) -> Key {
        self.reference
    }

    /// The reference key's frequency.
    #[must_use]
    pub const fn frequency(&self) -> Frequency {
        self.frequency
    }

    /// The degree of the scale that is the formal octave: the difference in
    /// pitch between one repetition of the pattern and the next. 0 stands for
    /// the scale's last pitch.
    #[must_use]
    pub const fn octave(&self) -> i64 {
        self.octave
    }

    /// The degree that each key of the pattern plays, from the middle key up,
    /// or none for a key left unmapped: no more than [`Mapping::size`] of them,
    /// the keys of the pattern after the last unmapped.
    #[must_use]
    pub fn degrees(&self) -> &[Option<i64>] {
        &self.degrees
    }
}

/// Why bytes are not a scale or a mapping, and where.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Error {
    /// The line, counted from 1; one past the last where the file ended too
    /// soon.
    pub line: usize,
    /// What was wrong there.
    pub message: &'static str,
}

impl fmt::Display for Error {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{} at line {}", self.message, self.line)
    }
}

impl std::error::Error for Error {}

/// The scale a `.scl` file holds.
///
/// # Errors
///
/// Where the file has no description or no count of notes, its count is not
/// a number, a pitch is neither cents nor a ratio of integers above 0 within
/// 64 bits, or fewer pitches follow than the count says.
pub fn parse_scl(bytes: &[u8]) -> Result<Scale, Error> {
    let mut lines = Lines::new(bytes);
    let description = lines
        .next_value(false)
        .ok_or_else(|| lines.end("a file without a description"))?;
    let count = lines
        .next_value(true)
        .ok_or_else(|| lines.end("a file without its count of notes"))?;
    let count: usize = integer(token(count))
        .and_then(|count| count.parse().ok())
        .ok_or_else(|| lines.error("a count of notes that is not a number"))?;
    let mut pitches = Vec::new();
    while pitches.len() < count {
        let line = lines
            .next_value(true)
            .ok_or_else(|| lines.end("fewer pitches than the count of notes"))?;
        pitches.push(pitch(token(line)).map_err(|message| lines.error(message))?);
    }
    Ok(Scale {
        description: text(description),
        pitches,
    })
}

/// The keyboard mapping a `.kbm` file holds.
///
/// # Errors
///
/// Where a line of the header is missing or is not what it should be, an
/// entry is neither an integer nor `x`, or the reference key is left unmapped.
pub fn parse_kbm(bytes: &[u8]) -> Result<Mapping, Error> {
    let mut lines = Lines::new(bytes);
    let mut header = |what: &'static str| -> Result<&[u8], Error> {
        lines.next_value(true).map(token).ok_or_else(|| lines.end(what))
    };
    let size = header("a file without its size of map")?;
    let first = header("a file without its first key")?;
    let last = header("a file without its last key")?;
    let middle = header("a file without its middle key")?;
    let reference = header("a file without its reference key")?;
    let frequency = header("a file without its reference frequency")?;
    let octave = header("a file without its degree of the formal octave")?;
    // The header's values, checked in its order: the line of each is the
    // header's first and the six after it, comments and blank lines aside.
    let numbers = lines.numbers.clone();
    let size: usize = check(
        integer(size).and_then(|size| size.parse().ok()),
        numbers[0],
        "a size of map that is not a count",
    )?;
    let first = check(key(first), numbers[1], "a first key that is not one of 0 to 127")?;
    let last = check(key(last), numbers[2], "a last key that is not one of 0 to 127")?;
    let middle = check(key(middle), numbers[3], "a middle key that is not one of 0 to 127")?;
    let reference = check(
        key(reference),
        numbers[4],
        "a reference key that is not one of 0 to 127",
    )?;
    let frequency = check(
        frequency_of(frequency),
        numbers[5],
        "a reference frequency that is not a number above 0",
    )?;
    let octave = check(
        integer(octave).and_then(|octave| octave.parse().ok()),
        numbers[6],
        "a degree of the formal octave that is not an integer",
    )?;
    let mut degrees = Vec::new();
    while degrees.len() < size {
        let Some(line) = lines.next_value(true) else {
            break;
        };
        let entry = token(line);
        let degree = if entry == b"x" {
            None
        } else {
            Some(
                integer(entry)
                    .and_then(|degree| degree.parse().ok())
                    .ok_or_else(|| lines.error("an entry that is neither a degree nor x"))?,
            )
        };
        degrees.push(degree);
    }
    // The entry that the reference key plays: none for a linear mapping, which
    // maps every key.
    if size > 0 {
        let offset = i64::from(reference.get()) - i64::from(middle.get());
        let entry = usize::try_from(offset.rem_euclid(i64::try_from(size).unwrap_or(i64::MAX))).ok();
        if entry.and_then(|entry| degrees.get(entry)).is_none_or(Option::is_none) {
            return Err(Error {
                line: numbers[4],
                message: "a reference key that the mapping leaves unmapped",
            });
        }
    }
    Ok(Mapping {
        size,
        first,
        last,
        middle,
        reference,
        frequency,
        octave,
        degrees,
    })
}

/// A value of a mapping's header, or the error of a message at its line.
fn check<T>(value: Option<T>, line: usize, message: &'static str) -> Result<T, Error> {
    value.ok_or(Error { line, message })
}

/// The lines of a file, and the numbers of those read.
struct Lines<'a> {
    /// The bytes not yet read.
    rest: &'a [u8],
    /// The number of the last line read, 0 before any.
    number: usize,
    /// The numbers of the lines that [`Lines::next_value`] gave, in order.
    numbers: Vec<usize>,
}

impl<'a> Lines<'a> {
    /// The lines of a file.
    const fn new(bytes: &'a [u8]) -> Self {
        Self {
            rest: bytes,
            number: 0,
            numbers: Vec::new(),
        }
    }

    /// The next line, without its end: none at the end of the file. A line
    /// ends at a line feed, a carriage return, or a carriage return and a line
    /// feed; an end of a line at the end of the file starts no line after it.
    fn next_line(&mut self) -> Option<&'a [u8]> {
        if self.rest.is_empty() {
            return None;
        }
        self.number += 1;
        let end = self.rest.iter().position(|&byte| byte == b'\n' || byte == b'\r');
        let (line, rest) = match end {
            Some(end) => {
                let (line, rest) = self.rest.split_at(end);
                let ending = if rest.starts_with(b"\r\n") { 2 } else { 1 };
                (line, rest.get(ending..).unwrap_or_default())
            }
            None => (self.rest, &[][..]),
        };
        self.rest = rest;
        Some(line)
    }

    /// The next line that is not a comment, nor, if `skip_blank`, blank: none
    /// at the end of the file.
    fn next_value(&mut self, skip_blank: bool) -> Option<&'a [u8]> {
        loop {
            let line = self.next_line()?;
            if line.starts_with(b"!") || (skip_blank && line.iter().all(|&byte| byte == b' ' || byte == b'\t')) {
                continue;
            }
            self.numbers.push(self.number);
            return Some(line);
        }
    }

    /// The error of a message at the last line read.
    const fn error(&self, message: &'static str) -> Error {
        Error {
            line: self.number,
            message,
        }
    }

    /// The error of a message at the end of the file, the line after its last.
    const fn end(&self, message: &'static str) -> Error {
        Error {
            line: self.number + 1,
            message,
        }
    }
}

/// The first run of a line's bytes that are not spaces or tabs.
fn token(line: &[u8]) -> &[u8] {
    let start = line
        .iter()
        .position(|&byte| byte != b' ' && byte != b'\t')
        .unwrap_or(line.len());
    let (_, line) = line.split_at(start);
    let end = line
        .iter()
        .position(|&byte| byte == b' ' || byte == b'\t')
        .unwrap_or(line.len());
    let (token, _) = line.split_at(end);
    token
}

/// The bytes as text: UTF-8 where they are, and otherwise ISO 8859-1, whose
/// every byte is the character of its code.
fn text(bytes: &[u8]) -> String {
    match std::str::from_utf8(bytes) {
        Ok(text) => text.to_owned(),
        Err(_) => bytes.iter().map(|&byte| char::from(byte)).collect(),
    }
}

/// Whether bytes are decimal digits, one at least.
fn digits(bytes: &[u8]) -> bool {
    !bytes.is_empty() && bytes.iter().all(u8::is_ascii_digit)
}

/// An integer's text, `-?[0-9]+`, if the token is one.
fn integer(token: &[u8]) -> Option<&str> {
    let unsigned = token.strip_prefix(b"-").unwrap_or(token);
    digits(unsigned).then(|| std::str::from_utf8(token).ok()).flatten()
}

/// A key of 0 to 127, if the token is one.
fn key(token: &[u8]) -> Option<Key> {
    integer(token).and_then(|key| key.parse().ok()).and_then(Key::new)
}

/// A frequency, `[0-9]+(\.[0-9]*)?` and above 0, if the token is one.
fn frequency_of(token: &[u8]) -> Option<Frequency> {
    let (whole, fraction) = match token.iter().position(|&byte| byte == b'.') {
        Some(point) => {
            let (whole, fraction) = token.split_at(point);
            (whole, fraction.get(1..).unwrap_or_default())
        }
        None => (token, &[][..]),
    };
    if !digits(whole) || !fraction.iter().all(u8::is_ascii_digit) {
        return None;
    }
    std::str::from_utf8(token)
        .ok()
        .and_then(|text| text.parse().ok())
        .and_then(Frequency::new)
}

/// The pitch a token writes, or why it writes none.
fn pitch(token: &[u8]) -> Result<Pitch, &'static str> {
    if let Some(point) = token.iter().position(|&byte| byte == b'.') {
        let (whole, fraction) = token.split_at(point);
        let fraction = fraction.get(1..).unwrap_or_default();
        let unsigned = whole.strip_prefix(b"-").unwrap_or(whole);
        if !digits(unsigned) || !fraction.iter().all(u8::is_ascii_digit) {
            return Err("a pitch in cents that is not -?[0-9]+.[0-9]*");
        }
        let cents = std::str::from_utf8(token)
            .ok()
            .and_then(|text| text.parse().ok())
            .and_then(Cents::new)
            .ok_or("a pitch in cents beyond binary64")?;
        return Ok(Pitch::Cents(cents));
    }
    if token.starts_with(b"-") {
        return Err("a negative ratio");
    }
    let (numerator, denominator) = match token.iter().position(|&byte| byte == b'/') {
        Some(slash) => {
            let (numerator, denominator) = token.split_at(slash);
            (numerator, denominator.get(1..).unwrap_or_default())
        }
        None => (token, &b"1"[..]),
    };
    if !digits(numerator) || !digits(denominator) {
        return Err("a pitch that is neither cents nor a ratio of integers");
    }
    let part = |digits: &[u8]| -> Result<NonZeroU64, &'static str> {
        let value: u64 = std::str::from_utf8(digits)
            .ok()
            .and_then(|text| text.parse().ok())
            .ok_or("a ratio with a part beyond 64 bits")?;
        NonZeroU64::new(value).ok_or("a ratio with a part of 0")
    };
    Ok(Pitch::Ratio(Ratio::new(part(numerator)?, part(denominator)?)))
}

#[cfg(test)]
mod tests {
    use super::{Error, Lines, Pitch, parse_kbm, parse_scl, token};

    #[test]
    fn lines_end_every_way() {
        let mut lines = Lines::new(b"a\nb\r\nc\rd\n\n");
        let mut read = Vec::new();
        while let Some(line) = lines.next_line() {
            read.push((lines.number, line.to_vec()));
        }
        let expected: Vec<(usize, Vec<u8>)> = vec![
            (1, b"a".to_vec()),
            (2, b"b".to_vec()),
            (3, b"c".to_vec()),
            (4, b"d".to_vec()),
            (5, Vec::new()),
        ];
        assert_eq!(read, expected);
        assert_eq!(token(b" \t 5/4 \tE\\"), b"5/4");
        assert_eq!(token(b"   "), b"");
    }

    #[test]
    fn scales_are_read() {
        let scale = parse_scl(
            b"! a comment\r\n\
              A scale of the tests\r\n\
              ! another\r\n\
               5\r\n\
              \r\n\
              81/64\r\n\
              408.0 cents\r\n\
              408.\r\n\
               5   E\\\r\n\
              -5.0\r\n\
              trailing lines are passed over",
        )
        .expect("a scale");
        assert_eq!(scale.description, "A scale of the tests");
        let pitches: Vec<String> = scale.pitches.iter().map(|pitch| format!("{pitch:?}")).collect();
        assert_eq!(
            pitches,
            [
                "Ratio(Ratio { numerator: 81, denominator: 64 })",
                "Cents(Cents(408.0))",
                "Cents(Cents(408.0))",
                "Ratio(Ratio { numerator: 5, denominator: 1 })",
                "Cents(Cents(-5.0))",
            ]
        );
        // An empty description, no notes, and a description of ISO 8859-1.
        let empty = parse_scl(b"\n0\n").expect("a scale");
        assert_eq!((empty.description.as_str(), empty.pitches.len()), ("", 0));
        let latin = parse_scl(b"Caf\xe9\n1\n2/1\n").expect("a scale");
        assert_eq!(latin.description, "Café");
        let utf8 = parse_scl("Café\n1\n2/1\n".as_bytes()).expect("a scale");
        assert_eq!(utf8.description, "Café");
        // 10/20 is kept as written, and a ratio of the largest parts.
        let large = parse_scl(b"\n2\n10/20\n18446744073709551615/1\n").expect("a scale");
        assert!(matches!(large.pitches[0], Pitch::Ratio(ratio) if ratio.numerator().get() == 10));
        assert!(matches!(large.pitches[1], Pitch::Ratio(ratio) if ratio.numerator().get() == u64::MAX));
    }

    #[test]
    fn what_is_not_a_scale_is_refused() {
        let cases: [(&[u8], usize, &str); 12] = [
            (b"", 1, "a file without a description"),
            (b"! only a comment\n", 2, "a file without a description"),
            (b"description\n", 2, "a file without its count of notes"),
            (b"description\nfive\n", 2, "a count of notes that is not a number"),
            (b"description\n2 notes\n", 3, "fewer pitches than the count of notes"),
            (b"description\n2\n100.0\n", 4, "fewer pitches than the count of notes"),
            (
                b"description\n1\n.5\n",
                3,
                "a pitch in cents that is not -?[0-9]+.[0-9]*",
            ),
            (b"description\n1\n-5/4\n", 3, "a negative ratio"),
            (b"description\n1\n0/1\n", 3, "a ratio with a part of 0"),
            (b"description\n1\n3/0\n", 3, "a ratio with a part of 0"),
            (
                b"description\n1\n18446744073709551616\n",
                3,
                "a ratio with a part beyond 64 bits",
            ),
            (
                b"description\n1\n1/2/3\n",
                3,
                "a pitch that is neither cents nor a ratio of integers",
            ),
        ];
        for (bytes, line, message) in cases {
            assert_eq!(
                parse_scl(bytes),
                Err(Error { line, message }),
                "{:?}",
                String::from_utf8_lossy(bytes)
            );
        }
        let huge = format!("description\n1\n{}.0\n", "9".repeat(400));
        assert_eq!(
            parse_scl(huge.as_bytes()),
            Err(Error {
                line: 3,
                message: "a pitch in cents beyond binary64"
            })
        );
        // A count beyond what the file holds asks for no allocation.
        assert_eq!(
            parse_scl(b"description\n18446744073709551615\n1/1\n").map_err(|error| error.line),
            Err(4)
        );
    }

    #[test]
    fn mappings_are_read() {
        let mapping = parse_kbm(
            b"! a mapping of twelve keys\n\
              12\n0\n127\n60\n69\n440.0\n12\n\
              ! the entries\n\
              0\n1\n2\n3\nx\n5\n6\n7\n8\n9\n10\n11\n\
              more lines are passed over\n",
        )
        .expect("a mapping");
        assert_eq!(mapping.size(), 12);
        assert_eq!(
            (mapping.first().get(), mapping.last().get(), mapping.middle().get()),
            (0, 127, 60)
        );
        assert_eq!(mapping.reference().get(), 69);
        assert_eq!(mapping.frequency().get().to_bits(), 440.0_f64.to_bits());
        assert_eq!(mapping.octave(), 12);
        assert_eq!(mapping.degrees().len(), 12);
        assert_eq!(mapping.degrees()[4], None);
        assert_eq!(mapping.degrees()[9], Some(9));
        // A linear mapping, and one whose entries stop short of its size, with
        // negative degrees.
        let linear = parse_kbm(b"0\n0\n127\n60\n60\n261.6255653\n0\n").expect("a mapping");
        assert_eq!((linear.size(), linear.degrees().len()), (0, 0));
        let short = parse_kbm(b"7\n0\n127\n60\n61\n440\n-7\n-1\n-2\n").expect("a mapping");
        assert_eq!(short.degrees(), [Some(-1), Some(-2)]);
        assert_eq!(short.octave(), -7);
    }

    #[test]
    fn what_is_not_a_mapping_is_refused() {
        let cases: [(&[u8], usize, &str); 9] = [
            (b"", 1, "a file without its size of map"),
            (
                b"12\n0\n127\n60\n69\n440.0\n",
                7,
                "a file without its degree of the formal octave",
            ),
            (
                b"-1\n0\n127\n60\n69\n440.0\n12\n",
                1,
                "a size of map that is not a count",
            ),
            (
                b"12\n0\n128\n60\n69\n440.0\n12\n",
                3,
                "a last key that is not one of 0 to 127",
            ),
            (
                b"12\n0\n127\n60\n69\n0\n12\n",
                6,
                "a reference frequency that is not a number above 0",
            ),
            (
                b"12\n0\n127\n60\n69\n4e2\n12\n",
                6,
                "a reference frequency that is not a number above 0",
            ),
            (
                b"12\n0\n127\n60\n69\n440.0\ntwelve\n",
                7,
                "a degree of the formal octave that is not an integer",
            ),
            (
                b"2\n0\n127\n60\n61\n440.0\n2\n0\ny\n",
                9,
                "an entry that is neither a degree nor x",
            ),
            (
                b"2\n0\n127\n60\n61\n440.0\n2\n0\nx\n",
                5,
                "a reference key that the mapping leaves unmapped",
            ),
        ];
        for (bytes, line, message) in cases {
            assert_eq!(
                parse_kbm(bytes),
                Err(Error { line, message }),
                "{:?}",
                String::from_utf8_lossy(bytes)
            );
        }
        // An entry left out at the end is unmapped, the reference's too, and a
        // reference below the middle key counts back from the pattern's end.
        assert!(parse_kbm(b"3\n0\n127\n60\n62\n440.0\n3\n0\n1\n").is_err());
        assert!(parse_kbm(b"3\n0\n127\n60\n59\n440.0\n3\n0\n1\n2\n").is_ok());
        assert!(parse_kbm(b"3\n0\n127\n60\n59\n440.0\n3\n0\n1\nx\n").is_err());
    }
}
