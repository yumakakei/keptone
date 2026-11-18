// SPDX-FileCopyrightText: 2026 Yuma Kakei <yumasansansan@gmail.com>
// SPDX-License-Identifier: GPL-3.0-or-later
//
// This file is part of Keptone. The SPDX lines name its copyright holder and its
// license, in the machine-readable form of the REUSE specification: the GNU
// General Public License, version 3 or any later version
// (LICENSES/GPL-3.0-or-later.txt). It follows kiral's json.rs, by the same
// author, under the same license.

//! JSON (RFC 8259), read by hand: the subset that Keptone's native state and
//! themes are written in (docs/DESIGN.md §11.1, docs/UI.md §6b).
//!
//! The parser takes any bytes. What is not UTF-8 and JSON is an error, never a
//! panic, and says the line and the column where reading stopped. So is a name
//! given twice in one object, which two readers could take two ways, and arrays
//! and objects nested deeper than the limit that `json.max_depth` sets
//! (docs/MATH.md §17). The parser keeps the arrays and objects it is in on a
//! stack of its own, on the heap, and so does dropping a value: no depth of
//! nesting takes more of the thread's stack than any other, whatever the limit
//! is set to. An object keeps its members in their order. A number is kept as
//! its text, beside the binary64 nearest to it, so that an integer of 64 bits,
//! such as a seed, can be read exactly.
//!
//! What the parser allocates is in proportion to the bytes it is given, and the
//! time it takes no more than in proportion to them times the logarithm of an
//! object's size: names are checked against each other once an object is read
//! whole, by sorting, not each against every earlier one. An object read keeps
//! the offsets of its opening brace and of each member's name as well, so that
//! what reads a value can say where in the text a member it refuses is, or
//! where the object that lacks one begins ([`line_and_column`]).
//!
//! [`write`](fn@write) writes a value as JSON text, a member of an object to a line, each
//! number as its text; [`write_canonical`] writes it in the canonical form of
//! RFC 8785 \[RFC8785\], with no white space, the members of each object in the
//! order of their names' UTF-16 code units and each number as ECMAScript
//! writes its binary64, which [`Number::from_f64`] gives as well. Neither takes
//! more of the thread's stack for a deep value than for a shallow one, and
//! what either writes, [`parse`] reads back as the value written.

use std::fmt;
use std::mem;

/// The default of `json.max_depth`: the deepest that arrays and objects may
/// nest. The native state and the themes nest a few levels deep.
pub const DEFAULT_MAX_DEPTH: usize = 64;

/// A JSON value.
#[derive(Debug, Clone, PartialEq)]
pub enum Value {
    /// `null`.
    Null,
    /// `true` or `false`.
    Bool(bool),
    /// A number.
    Number(Number),
    /// A string.
    String(String),
    /// An array.
    Array(Vec<Value>),
    /// An object.
    Object(Object),
}

impl Drop for Value {
    /// Drops the values within a level at a time, from a stack of the heap, so
    /// that dropping a deep value takes no more of the thread's stack than a
    /// shallow one.
    fn drop(&mut self) {
        let mut within = Vec::new();
        self.take_within(&mut within);
        while let Some(mut value) = within.pop() {
            value.take_within(&mut within);
        }
    }
}

/// A number as it is written, `-? (0 | [1-9][0-9]*) (. [0-9]+)? ([eE] [+-]?
/// [0-9]+)?`, and the binary64 nearest to it.
#[derive(Debug, Clone, PartialEq)]
pub struct Number {
    /// The number's text.
    text: String,
    /// The binary64 nearest to it, infinite beyond binary64's range.
    value: f64,
}

impl Number {
    /// The number's text.
    #[must_use]
    pub fn as_text(&self) -> &str {
        &self.text
    }

    /// The number as an unsigned integer: none where it has a fraction or an
    /// exponent, is negative, or is above `u64::MAX`.
    #[must_use]
    pub fn as_u64(&self) -> Option<u64> {
        self.text.parse().ok()
    }

    /// The number as a signed integer: none where it has a fraction or an
    /// exponent, or is out of the range of `i64`.
    #[must_use]
    pub fn as_i64(&self) -> Option<i64> {
        self.text.parse().ok()
    }

    /// The binary64 nearest to the number, rounded to nearest with ties to
    /// even; infinite beyond binary64's range, and −0 for a negative zero.
    #[must_use]
    pub const fn as_f64(&self) -> f64 {
        self.value
    }

    /// The number of a binary64, if it is finite, written as ECMAScript's
    /// `Number::toString` writes it \[ECMA-262\] §6.1.6.1.20, which RFC 8785 takes
    /// for its canonical form \[RFC8785\] §3.2.2.3: the fewest significant
    /// digits that read back as the binary64, the nearest to it where several
    /// do; in fixed notation from 10⁻⁶ to below 10²¹, as 0.000001 and
    /// 100000000000000000000, and beyond with an exponent and its sign, as
    /// 1e-7 and 1e+21; and −0 as 0, which reads back as +0, the number's value.
    #[must_use]
    pub fn from_f64(value: f64) -> Option<Self> {
        value.is_finite().then(|| Self {
            text: ecmascript(value),
            // Adding +0 takes −0 to +0 and leaves every other number as it is.
            value: value + 0.0,
        })
    }

    /// The number of an unsigned integer, written whole.
    #[must_use]
    pub fn from_u64(integer: u64) -> Self {
        let text = integer.to_string();
        // Rust reads the digits as the binary64 nearest to them.
        let value = text.parse().unwrap_or(f64::INFINITY);
        Self { text, value }
    }
}

/// The text of a finite binary64 as ECMAScript writes it ([`Number::from_f64`]).
fn ecmascript(value: f64) -> String {
    if value == 0.0 {
        return "0".to_owned();
    }
    let (digits, n) = shortest(value.abs());
    let k = i32::try_from(digits.len()).unwrap_or(i32::MAX);
    let zeros = |count: i32| std::iter::repeat_n('0', usize::try_from(count).unwrap_or(0));
    let mut text = String::new();
    if value < 0.0 {
        text.push('-');
    }
    if k <= n && n <= 21 {
        text.push_str(&digits);
        text.extend(zeros(n - k));
    } else if 0 < n && n <= 21 {
        let (whole, fraction) = digits
            .split_at_checked(usize::try_from(n).unwrap_or(0))
            .unwrap_or((&digits, ""));
        text.push_str(whole);
        text.push('.');
        text.push_str(fraction);
    } else if -6 < n && n <= 0 {
        text.push_str("0.");
        text.extend(zeros(-n));
        text.push_str(&digits);
    } else {
        let (first, rest) = digits.split_at_checked(1).unwrap_or((&digits, ""));
        text.push_str(first);
        if !rest.is_empty() {
            text.push('.');
            text.push_str(rest);
        }
        let exponent = n - 1;
        text.push_str(if exponent < 0 { "e-" } else { "e+" });
        text.push_str(&exponent.unsigned_abs().to_string());
    }
    text
}

/// ECMA-262's digits of a binary64 above 0, s of k digits, and its n, the
/// binary64 being nearest to 0.d₁d₂…dₖ × 10ⁿ: the fewest digits that read
/// back as it and, of those, the nearest to it; of two as near, the even.
///
/// Rust gives the fewest and nearest digits, but of two as near the upper,
/// as for 1424953923781206.25, written …206.2 by ECMAScript and …206.3 by
/// Rust \[RFC8785\] Appendix B. Two are as near where the binary64 is exactly
/// halfway between them, which [`is_exactly`] settles without rounding; the
/// even then takes the odd's place if it reads back as the binary64 too.
fn shortest(value: f64) -> (String, i32) {
    // d.ddd and the exponent of the first digit, which is n − 1.
    let exponential = format!("{value:e}");
    let (mantissa, exponent) = exponential.split_once('e').unwrap_or((&exponential, "0"));
    let digits: String = mantissa.chars().filter(char::is_ascii_digit).collect();
    let n = exponent.parse::<i32>().unwrap_or(0) + 1;
    let k = i32::try_from(digits.len()).unwrap_or(i32::MAX);
    // A binary64 has 17 significant digits at the most, which 64 bits hold
    // ten times over.
    let Some(s) = digits.parse::<u64>().ok().filter(|s| !s.is_multiple_of(2)) else {
        return (digits, n);
    };
    for (other, halfway) in [
        (s - 1, s.checked_mul(10).map(|tens| tens - 5)),
        (s + 1, s.checked_mul(10).map(|tens| tens + 5)),
    ] {
        let other_digits = other.to_string();
        let reads_back = || {
            format!("{other_digits}e{}", n - k)
                .parse::<f64>()
                .is_ok_and(|other| other.to_bits() == value.to_bits())
        };
        if other_digits.len() == digits.len()
            && halfway.is_some_and(|halfway| is_exactly(value, halfway, n - k - 1))
            && reads_back()
        {
            return (other_digits, n);
        }
    }
    (digits, n)
}

/// Whether a binary64 above 0 is exactly d × 10ᵖ, for d above 0.
///
/// The binary64 is m × 2^q, m and q of its bits. Two products of powers of 2
/// and 5 and of integers prime to 10 are equal exactly where those integers
/// are, and the powers of 2 and of 5 on both sides: m = 2ᵃ 5ᵇ r and
/// d = 2ᶜ 5ᶠ t are equal times 2^q and 10ᵖ where r = t, a + q = c + p and
/// b = f + p, by the uniqueness of factorization into primes.
fn is_exactly(value: f64, digits: u64, power: i32) -> bool {
    let bits = value.to_bits();
    let field = i32::try_from(bits >> 52 & 0x7ff).unwrap_or(0);
    let fraction = bits & ((1 << 52) - 1);
    // A subnormal's m is its fraction alone, and its q that of the smallest
    // normal binary64's.
    let (significand, exponent) = if field == 0 {
        (fraction, -1074)
    } else {
        (fraction | 1 << 52, field - 1075)
    };
    let (significand_twos, significand_fives, significand_rest) = split_tens(significand);
    let (digits_twos, digits_fives, digits_rest) = split_tens(digits);
    significand != 0
        && digits != 0
        && significand_rest == digits_rest
        && significand_twos + exponent == digits_twos + power
        && significand_fives == digits_fives + power
}

/// An integer above 0 as 2ᵃ 5ᵇ r, r prime to 10: a, b and r.
fn split_tens(integer: u64) -> (i32, i32, u64) {
    if integer == 0 {
        return (0, 0, 0);
    }
    let twos = integer.trailing_zeros();
    let mut rest = integer >> twos;
    let mut fives = 0;
    while rest.is_multiple_of(5) {
        rest /= 5;
        fives += 1;
    }
    (i32::try_from(twos).unwrap_or(0), fives, rest)
}

/// An object: its members in their order, each name once, and where it and
/// each member's name lie in the text it was read from. Two objects are equal
/// if their members are, wherever they were read from.
#[derive(Debug, Clone, Default)]
pub struct Object {
    /// The members, in their order.
    members: Vec<(String, Value)>,
    /// The offset of each member's name in the text, 0 for a member not read
    /// from one.
    offsets: Vec<usize>,
    /// The offset of the object's opening brace in the text, 0 for an object
    /// not read from one.
    start: usize,
}

impl PartialEq for Object {
    fn eq(&self, other: &Self) -> bool {
        self.members == other.members
    }
}

impl Object {
    /// An object of no members.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Adds a member after the others. What writes the object writes the
    /// members as they were added, a name given twice among them.
    pub fn push(&mut self, name: impl Into<String>, value: Value) {
        self.members.push((name.into(), value));
        self.offsets.push(0);
    }

    /// The value of a member.
    #[must_use]
    pub fn get(&self, name: &str) -> Option<&Value> {
        self.members
            .iter()
            .find(|(member, _)| member == name)
            .map(|(_, value)| value)
    }

    /// The offset of a member's name in the text the object was read from: of
    /// its opening quote.
    #[must_use]
    pub fn offset(&self, name: &str) -> Option<usize> {
        let place = self.members.iter().position(|(member, _)| member == name)?;
        self.offsets.get(place).copied()
    }

    /// The offset of the object's opening brace in the text it was read from,
    /// 0 for an object made otherwise.
    #[must_use]
    pub const fn start(&self) -> usize {
        self.start
    }

    /// The members, in their order.
    pub fn iter(&self) -> impl Iterator<Item = (&str, &Value)> {
        self.members.iter().map(|(name, value)| (name.as_str(), value))
    }

    /// The members, in their order, each with the offset of its name.
    pub fn iter_offsets(&self) -> impl Iterator<Item = (&str, &Value, usize)> {
        self.iter()
            .zip(self.offsets.iter().copied())
            .map(|((name, value), offset)| (name, value, offset))
    }

    /// The number of members.
    #[must_use]
    pub fn len(&self) -> usize {
        self.members.len()
    }

    /// Whether the object has no members.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.members.is_empty()
    }
}

impl Value {
    /// The string, where the value is one.
    #[must_use]
    pub fn as_str(&self) -> Option<&str> {
        match self {
            Self::String(text) => Some(text),
            _ => None,
        }
    }

    /// The object, where the value is one.
    #[must_use]
    pub fn as_object(&self) -> Option<&Object> {
        match self {
            Self::Object(object) => Some(object),
            _ => None,
        }
    }

    /// The array, where the value is one.
    #[must_use]
    pub fn as_array(&self) -> Option<&[Value]> {
        match self {
            Self::Array(values) => Some(values),
            _ => None,
        }
    }

    /// The boolean, where the value is one.
    #[must_use]
    pub fn as_bool(&self) -> Option<bool> {
        match self {
            Self::Bool(value) => Some(*value),
            _ => None,
        }
    }

    /// The number, where the value is one.
    #[must_use]
    pub fn as_number(&self) -> Option<&Number> {
        match self {
            Self::Number(number) => Some(number),
            _ => None,
        }
    }

    /// The number as an unsigned integer, where the value is one that is.
    #[must_use]
    pub fn as_u64(&self) -> Option<u64> {
        self.as_number().and_then(Number::as_u64)
    }

    /// The number as a binary64, where the value is a number.
    #[must_use]
    pub fn as_f64(&self) -> Option<f64> {
        self.as_number().map(Number::as_f64)
    }

    /// Moves the values within an array or an object onto `within`, leaving it
    /// empty.
    fn take_within(&mut self, within: &mut Vec<Self>) {
        match self {
            Self::Array(values) => within.append(values),
            Self::Object(object) => within.extend(object.members.drain(..).map(|(_, value)| value)),
            _ => {}
        }
    }
}

/// Why bytes are not JSON of the subset, and where.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Error {
    /// The offset of the byte where reading stopped.
    pub offset: usize,
    /// Its line, counted from 1: one more than the line feeds before it.
    pub line: usize,
    /// Its column, counted from 1 in characters from the start of its line.
    pub column: usize,
    /// What was wrong there.
    pub message: &'static str,
}

impl Error {
    /// The error of a message at an offset of bytes whose first `offset` are
    /// UTF-8.
    fn at(bytes: &[u8], offset: usize, message: &'static str) -> Self {
        let before = bytes.get(..offset).unwrap_or(bytes);
        let line = before.split(|&byte| byte == b'\n').count();
        let current = before.rsplit(|&byte| byte == b'\n').next().unwrap_or(before);
        // Every byte of a character but its first is of the form 10xxxxxx.
        let column = 1 + current.iter().filter(|&&byte| byte & 0xc0 != 0x80).count();
        Self {
            offset,
            line,
            column,
            message,
        }
    }
}

impl fmt::Display for Error {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "{} at line {}, column {}",
            self.message, self.line, self.column
        )
    }
}

impl std::error::Error for Error {}

/// The line and the column of an offset in a text, counted from 1 as an
/// [`Error`] counts them: the line by the line feeds before it, the column in
/// characters.
#[must_use]
pub fn line_and_column(bytes: &[u8], offset: usize) -> (usize, usize) {
    let place = Error::at(bytes, offset, "");
    (place.line, place.column)
}

/// The value of a JSON text: one value, with white space around it and nothing
/// else, its arrays and objects nested `max_depth` deep at the most.
///
/// # Errors
///
/// Where the bytes are not UTF-8 and JSON, name a member of an object twice,
/// or nest deeper than `max_depth`.
pub fn parse(bytes: &[u8], max_depth: usize) -> Result<Value, Error> {
    if let Err(error) = std::str::from_utf8(bytes) {
        return Err(Error::at(bytes, error.valid_up_to(), "bytes that are not UTF-8"));
    }
    let mut reader = Reader { bytes, at: 0 };
    // The arrays and objects the reader is in, the innermost last.
    let mut open: Vec<Open> = Vec::new();
    reader.skip_space();
    loop {
        // A value begins: a word, a number or a string, read whole, or an array
        // or an object, opened, unless it is empty.
        let mut value = match reader.peek() {
            Some(bracket @ (b'[' | b'{')) => {
                if open.len() >= max_depth {
                    return Err(reader.error("arrays and objects nested deeper than json.max_depth"));
                }
                let start = reader.at;
                reader.at += 1;
                reader.skip_space();
                if bracket == b'[' {
                    if reader.peek() == Some(b']') {
                        reader.at += 1;
                        Value::Array(Vec::new())
                    } else {
                        open.push(Open::Array(Vec::new()));
                        continue;
                    }
                } else if reader.peek() == Some(b'}') {
                    reader.at += 1;
                    Value::Object(Object { start, ..Object::new() })
                } else {
                    let (name, offset) = reader.name()?;
                    open.push(Open::Object(OpenObject {
                        members: Vec::new(),
                        offsets: Vec::new(),
                        name,
                        offset,
                        start,
                    }));
                    continue;
                }
            }
            _ => reader.scalar()?,
        };
        // The value is whole. It goes into the array or the object it is in,
        // and each array or object that it completes into the one it is in in
        // turn, until one goes on with a comma, or the text ends.
        loop {
            let Some(container) = open.pop() else {
                reader.skip_space();
                if reader.at != bytes.len() {
                    return Err(reader.error("something after the value"));
                }
                return Ok(value);
            };
            reader.skip_space();
            match container {
                Open::Array(mut values) => {
                    values.push(value);
                    match reader.peek() {
                        Some(b',') => {
                            reader.at += 1;
                            reader.skip_space();
                            open.push(Open::Array(values));
                            break;
                        }
                        Some(b']') => {
                            reader.at += 1;
                            value = Value::Array(values);
                        }
                        _ => return Err(reader.error("an array's value followed by neither a comma nor its end")),
                    }
                }
                Open::Object(mut object) => {
                    object.members.push((mem::take(&mut object.name), value));
                    object.offsets.push(object.offset);
                    match reader.peek() {
                        Some(b',') => {
                            reader.at += 1;
                            reader.skip_space();
                            (object.name, object.offset) = reader.name()?;
                            open.push(Open::Object(object));
                            break;
                        }
                        Some(b'}') => {
                            reader.at += 1;
                            if let Some(offset) = object.repeated() {
                                return Err(Error::at(bytes, offset, "a name given twice in one object"));
                            }
                            value = Value::Object(Object {
                                members: object.members,
                                offsets: object.offsets,
                                start: object.start,
                            });
                        }
                        _ => {
                            return Err(reader.error("an object's member followed by neither a comma nor its end"));
                        }
                    }
                }
            }
        }
    }
}

/// An array or an object that the reader is in.
enum Open {
    /// An array, and the values read of it.
    Array(Vec<Value>),
    /// An object, and the members read of it.
    Object(OpenObject),
}

/// An object that the reader is in.
struct OpenObject {
    /// The members read.
    members: Vec<(String, Value)>,
    /// The offset of each member's name.
    offsets: Vec<usize>,
    /// The name of the member whose value is being read.
    name: String,
    /// The offset of that name.
    offset: usize,
    /// The offset of the object's opening brace.
    start: usize,
}

impl OpenObject {
    /// The offset of the earliest name that an earlier one repeats, if any.
    fn repeated(&self) -> Option<usize> {
        let mut order: Vec<usize> = (0..self.members.len()).collect();
        // By name, and within a name by place, so that the second of each run
        // of equal names is the earliest that repeats one before it.
        order.sort_unstable_by(|&i, &j| self.members[i].0.cmp(&self.members[j].0).then(i.cmp(&j)));
        order
            .windows(2)
            .filter(|pair| self.members[pair[0]].0 == self.members[pair[1]].0)
            .map(|pair| self.offsets[pair[1]])
            .min()
    }
}

/// The bytes of a JSON text, and where in them reading has come to.
struct Reader<'a> {
    /// The text, UTF-8.
    bytes: &'a [u8],
    /// The offset of the next byte to read.
    at: usize,
}

impl Reader<'_> {
    /// The error of a message at the byte where reading has come to.
    fn error(&self, message: &'static str) -> Error {
        Error::at(self.bytes, self.at, message)
    }

    /// The next byte, if any.
    fn peek(&self) -> Option<u8> {
        self.bytes.get(self.at).copied()
    }

    /// Passes over white space.
    fn skip_space(&mut self) {
        while let Some(b' ' | b'\t' | b'\n' | b'\r') = self.peek() {
            self.at += 1;
        }
    }

    /// Whether the bytes from where reading has come to begin with these.
    fn looking_at(&self, word: &[u8]) -> bool {
        self.bytes.get(self.at..).is_some_and(|rest| rest.starts_with(word))
    }

    /// A member's name in quotes, its offset, and the colon after it.
    fn name(&mut self) -> Result<(String, usize), Error> {
        if self.peek() != Some(b'"') {
            return Err(self.error("a member without a name in quotes"));
        }
        let offset = self.at;
        let name = self.string()?;
        self.skip_space();
        if self.peek() != Some(b':') {
            return Err(self.error("a member's name without a colon after it"));
        }
        self.at += 1;
        self.skip_space();
        Ok((name, offset))
    }

    /// A value that is not an array or an object: a word, a number or a
    /// string.
    fn scalar(&mut self) -> Result<Value, Error> {
        match self.peek() {
            None => Err(self.error("the end where a value was expected")),
            Some(b'"') => self.string().map(Value::String),
            Some(b't') => self.word(b"true", Value::Bool(true)),
            Some(b'f') => self.word(b"false", Value::Bool(false)),
            Some(b'n') => self.word(b"null", Value::Null),
            Some(b'-' | b'0'..=b'9') => self.number().map(Value::Number),
            Some(_) => Err(self.error("a character that begins no value")),
        }
    }

    /// `true`, `false` or `null`.
    fn word(&mut self, word: &[u8], value: Value) -> Result<Value, Error> {
        if self.looking_at(word) {
            self.at += word.len();
            Ok(value)
        } else {
            Err(self.error("a word that is not true, false or null"))
        }
    }

    /// Passes over decimal digits, and says how many.
    fn digits(&mut self) -> usize {
        let start = self.at;
        while let Some(b'0'..=b'9') = self.peek() {
            self.at += 1;
        }
        self.at - start
    }

    /// A number.
    fn number(&mut self) -> Result<Number, Error> {
        let start = self.at;
        if self.peek() == Some(b'-') {
            self.at += 1;
        }
        match self.peek() {
            Some(b'0') => self.at += 1,
            Some(b'1'..=b'9') => {
                self.digits();
            }
            _ => return Err(self.error("a minus sign without digits")),
        }
        if self.peek() == Some(b'.') {
            self.at += 1;
            if self.digits() == 0 {
                return Err(self.error("a decimal point without digits after it"));
            }
        }
        if let Some(b'e' | b'E') = self.peek() {
            self.at += 1;
            if let Some(b'+' | b'-') = self.peek() {
                self.at += 1;
            }
            if self.digits() == 0 {
                return Err(self.error("an exponent without digits"));
            }
        }
        // The text is of ASCII digits and signs, which Rust reads as a binary64
        // correctly rounded, whatever its length.
        let text = self
            .bytes
            .get(start..self.at)
            .and_then(|text| std::str::from_utf8(text).ok())
            .ok_or_else(|| self.error("a number that is not one"))?;
        let value = text.parse().map_err(|_| self.error("a number that is not one"))?;
        Ok(Number {
            text: text.to_owned(),
            value,
        })
    }

    /// Four hexadecimal digits of a `\u` escape.
    fn hex4(&mut self) -> Result<u32, Error> {
        let Some(digits) = self.bytes.get(self.at..self.at + 4) else {
            return Err(self.error("an escape of fewer than four hexadecimal digits"));
        };
        let mut value = 0;
        for &digit in digits {
            let nibble = char::from(digit)
                .to_digit(16)
                .ok_or_else(|| self.error("an escape of fewer than four hexadecimal digits"))?;
            value = value * 16 + nibble;
        }
        self.at += 4;
        Ok(value)
    }

    /// A string in quotes.
    fn string(&mut self) -> Result<String, Error> {
        self.at += 1;
        let mut text = String::new();
        loop {
            let start = self.at;
            while let Some(byte) = self.peek() {
                if byte == b'"' || byte == b'\\' || byte < 0x20 {
                    break;
                }
                self.at += 1;
            }
            // The run is of whole characters: the text is UTF-8, and the run
            // stops only at ASCII bytes.
            let run = self
                .bytes
                .get(start..self.at)
                .and_then(|run| std::str::from_utf8(run).ok())
                .ok_or_else(|| self.error("bytes that are not UTF-8"))?;
            text.push_str(run);
            match self.peek() {
                None => return Err(self.error("a string without its closing quote")),
                Some(b'"') => {
                    self.at += 1;
                    return Ok(text);
                }
                Some(b'\\') => {
                    self.at += 1;
                    let escape = self.peek().ok_or_else(|| self.error("an escape at the end"))?;
                    self.at += 1;
                    match escape {
                        b'"' => text.push('"'),
                        b'\\' => text.push('\\'),
                        b'/' => text.push('/'),
                        b'b' => text.push('\u{8}'),
                        b'f' => text.push('\u{c}'),
                        b'n' => text.push('\n'),
                        b'r' => text.push('\r'),
                        b't' => text.push('\t'),
                        b'u' => text.push(self.unicode_escape()?),
                        _ => return Err(self.error("an escape that JSON does not have")),
                    }
                }
                Some(_) => return Err(self.error("a control character in a string")),
            }
        }
    }

    /// The character of a `\u` escape, a surrogate pair's two escapes together.
    fn unicode_escape(&mut self) -> Result<char, Error> {
        let first = self.hex4()?;
        let code = match first {
            0xd800..=0xdbff => {
                if !self.looking_at(b"\\u") {
                    return Err(self.error("a high surrogate without a low one after it"));
                }
                self.at += 2;
                let second = self.hex4()?;
                if !(0xdc00..=0xdfff).contains(&second) {
                    return Err(self.error("a high surrogate without a low one after it"));
                }
                0x10000 + ((first - 0xd800) << 10) + (second - 0xdc00)
            }
            0xdc00..=0xdfff => return Err(self.error("a low surrogate without a high one before it")),
            _ => first,
        };
        char::from_u32(code).ok_or_else(|| self.error("an escape of no character"))
    }
}

/// A value as JSON text: each member of an object on a line of its own,
/// indented two spaces a level, an array of nothing but numbers, strings,
/// words and empty arrays and objects on one line, each number as its text,
/// and a line feed at the end.
///
/// Each line is indented as deep as it lies, so that the text of a value
/// nested n levels deep is of the order of n² bytes: the form is for what
/// people read, which nests a few levels, as the native state and the themes
/// do. [`write_canonical`] writes any value in a text in proportion to it.
#[must_use]
pub fn write(value: &Value) -> String {
    let mut text = String::new();
    write_into(value, Form::Lines, &mut text);
    text.push('\n');
    text
}

/// A value in the canonical form of RFC 8785, the JSON Canonicalization
/// Scheme \[RFC8785\] §3.2: no white space; the members of each object in the
/// order of their names' UTF-16 code units, compared one by one; each number
/// as ECMAScript writes its binary64 ([`Number::from_f64`]), but one beyond
/// binary64's range, which the scheme leaves without a form, as its text; and
/// each string as [`write`](fn@write) writes it, a quote, a backslash and a control
/// character escaped and nothing else, as the scheme asks. Values that differ
/// only in the order of their members, in white space or in how a number is
/// written give the same text, so that a digest of it identifies what they
/// hold.
#[must_use]
pub fn write_canonical(value: &Value) -> String {
    let mut text = String::new();
    write_into(value, Form::Canonical, &mut text);
    text
}

/// How [`write_into`] writes a value.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Form {
    /// As [`write`](fn@write) does.
    Lines,
    /// As [`write_canonical`] does.
    Canonical,
}

/// An array or an object that the writer is in.
enum Within<'a> {
    /// An array: its values to come, whether one has been written, and
    /// whether it is written on one line.
    Array(std::slice::Iter<'a, Value>, bool, bool),
    /// An object: its members to come, in the order they are written, and
    /// whether one has been written.
    Object(std::vec::IntoIter<&'a (String, Value)>, bool),
}

/// Writes a value, one level at a time from a stack of the heap, so that a
/// deep value takes no more of the thread's stack than a shallow one.
fn write_into(value: &Value, form: Form, text: &mut String) {
    let lines = form == Form::Lines;
    let mut within: Vec<Within<'_>> = Vec::new();
    let mut next = Some(value);
    loop {
        if let Some(value) = next.take() {
            match value {
                Value::Array(values) if !values.is_empty() => {
                    text.push('[');
                    let one_line = !lines || values.iter().all(is_flat);
                    within.push(Within::Array(values.iter(), false, one_line));
                }
                Value::Object(object) if !object.is_empty() => {
                    text.push('{');
                    let mut members: Vec<&(String, Value)> = object.members.iter().collect();
                    if form == Form::Canonical {
                        members.sort_by(|(one, _), (other, _)| one.encode_utf16().cmp(other.encode_utf16()));
                    }
                    within.push(Within::Object(members.into_iter(), false));
                }
                flat => write_flat(flat, form, text),
            }
        }
        let level = within.len();
        let Some(innermost) = within.last_mut() else {
            return;
        };
        match innermost {
            Within::Array(values, written, one_line) => {
                let one_line = *one_line;
                if let Some(value) = values.next() {
                    if *written {
                        text.push(',');
                        if lines && one_line {
                            text.push(' ');
                        }
                    }
                    if !one_line {
                        new_line(text, level);
                    }
                    *written = true;
                    next = Some(value);
                } else {
                    within.pop();
                    if !one_line {
                        new_line(text, level - 1);
                    }
                    text.push(']');
                }
            }
            Within::Object(members, written) => {
                if let Some((name, value)) = members.next() {
                    if *written {
                        text.push(',');
                    }
                    if lines {
                        new_line(text, level);
                    }
                    *written = true;
                    write_string(name, text);
                    text.push_str(if lines { ": " } else { ":" });
                    next = Some(value);
                } else {
                    within.pop();
                    if lines {
                        new_line(text, level - 1);
                    }
                    text.push('}');
                }
            }
        }
    }
}

/// Whether a value is written on one line within an array: one that holds no
/// value within it.
fn is_flat(value: &Value) -> bool {
    match value {
        Value::Array(values) => values.is_empty(),
        Value::Object(object) => object.is_empty(),
        _ => true,
    }
}

/// Starts a line indented two spaces a level.
fn new_line(text: &mut String, level: usize) {
    text.push('\n');
    text.extend(std::iter::repeat_n(' ', 2 * level));
}

/// Writes a value that holds no value within it.
fn write_flat(value: &Value, form: Form, text: &mut String) {
    match value {
        Value::Null => text.push_str("null"),
        Value::Bool(true) => text.push_str("true"),
        Value::Bool(false) => text.push_str("false"),
        Value::Number(number) => match (form, Number::from_f64(number.value)) {
            (Form::Canonical, Some(canonical)) => text.push_str(&canonical.text),
            _ => text.push_str(&number.text),
        },
        Value::String(string) => write_string(string, text),
        Value::Array(_) => text.push_str("[]"),
        Value::Object(_) => text.push_str("{}"),
    }
}

/// Writes a string in quotes: a quote, a backslash and a control character
/// escaped, as RFC 8259 asks, and every other character as it is.
fn write_string(string: &str, text: &mut String) {
    text.push('"');
    for character in string.chars() {
        match character {
            '"' => text.push_str("\\\""),
            '\\' => text.push_str("\\\\"),
            '\n' => text.push_str("\\n"),
            '\r' => text.push_str("\\r"),
            '\t' => text.push_str("\\t"),
            '\u{8}' => text.push_str("\\b"),
            '\u{c}' => text.push_str("\\f"),
            control if control < ' ' => {
                let code = u32::from(control);
                text.push_str("\\u00");
                text.extend([code >> 4, code & 0xf].map(|nibble| char::from_digit(nibble, 16).unwrap_or('0')));
            }
            other => text.push(other),
        }
    }
    text.push('"');
}

#[cfg(test)]
mod tests {
    use super::{DEFAULT_MAX_DEPTH, Number, Object, Value, line_and_column, parse, write, write_canonical};

    fn number(text: &str) -> Value {
        Value::Number(Number {
            text: text.to_owned(),
            value: text.parse().expect("a number"),
        })
    }

    fn read(text: &[u8]) -> Value {
        parse(text, DEFAULT_MAX_DEPTH).expect("JSON")
    }

    #[test]
    fn values_are_read() {
        assert_eq!(parse(b" null ", DEFAULT_MAX_DEPTH), Ok(Value::Null));
        assert_eq!(read(b"true"), Value::Bool(true));
        assert_eq!(
            read(b"[1, -0.5e+3, 0]"),
            Value::Array(vec![number("1"), number("-0.5e+3"), number("0")])
        );
        let value = read(br#"{"a": {"b": []}, "c": "d", "e": [{}, [[]], {"f": null}]}"#);
        let object = value.as_object().expect("an object");
        assert_eq!(object.len(), 3);
        assert_eq!(object.get("c").and_then(Value::as_str), Some("d"));
        assert_eq!(object.iter().map(|(name, _)| name).collect::<Vec<_>>(), ["a", "c", "e"]);
        let e = object.get("e").and_then(Value::as_array).expect("an array");
        assert_eq!(e.len(), 3);
        assert_eq!(e[2].as_object().and_then(|f| f.get("f")), Some(&Value::Null));
    }

    #[test]
    fn strings_take_their_escapes() {
        let value = read(br#""a\"\\\/\b\f\n\r\t\u00e9\u3042\ud83d\ude00""#);
        assert_eq!(value.as_str(), Some("a\"\\/\u{8}\u{c}\n\r\té\u{3042}\u{1f600}"));
        assert_eq!(read("\"かな\"".as_bytes()).as_str(), Some("かな"));
    }

    #[test]
    fn numbers_are_read_as_they_are_asked_for() {
        assert_eq!(read(b"18446744073709551615").as_u64(), Some(u64::MAX));
        assert_eq!(read(b"18446744073709551616").as_u64(), None);
        assert_eq!(read(b"1.5").as_u64(), None);
        assert_eq!(read(b"1e2").as_u64(), None);
        assert_eq!(read(b"1e400").as_f64(), Some(f64::INFINITY));
        assert_eq!(read(b"-12").as_number().and_then(Number::as_i64), Some(-12));
        assert_eq!(read(b"0.1").as_f64(), Some(0.1));
        assert_eq!(read(b"-0").as_f64().map(f64::to_bits), Some((-0.0_f64).to_bits()));
        assert_eq!(read(b"2.5e-324").as_f64(), Some(f64::from_bits(1)));
        assert_eq!(read(b"9007199254740993").as_f64(), Some(9_007_199_254_740_992.0));
        assert_eq!(read(b"9007199254740993").as_u64(), Some(9_007_199_254_740_993));
        assert_eq!(read(b"1E+2").as_number().map(Number::as_text), Some("1E+2"));
    }

    #[test]
    fn what_is_not_json_is_refused() {
        for text in [
            &b""[..],
            b" ",
            b"nul",
            b"[1,]",
            b"[1 2]",
            b"{\"a\" 1}",
            b"{\"a\":1,}",
            b"{\"a\":1 \"b\":2}",
            b"{1:2}",
            b"01",
            b"1.",
            b".5",
            b"-",
            b"1e",
            b"+1",
            b"\"\x01\"",
            b"\"\\x\"",
            b"\"\\u12\"",
            b"\"\\ud800\"",
            b"\"\\udc00\"",
            b"\"\\ud800\\u0041\"",
            b"\"unterminated",
            b"[1] 2",
            b"[",
            b"{",
            b"{\"a\":",
            b"{\"a\":1,\"a\":2}",
            b"\xff",
            b"\"\xc3\"",
            b"\xef\xbb\xbf{}",
        ] {
            assert!(
                parse(text, DEFAULT_MAX_DEPTH).is_err(),
                "{:?} was taken",
                String::from_utf8_lossy(text)
            );
        }
    }

    #[test]
    fn errors_say_where() {
        let error = parse(b"{\n  \"a\": 1,\n  \"b\" 2\n}", DEFAULT_MAX_DEPTH).expect_err("not JSON");
        assert_eq!((error.offset, error.line, error.column), (18, 3, 7));
        assert_eq!(error.message, "a member's name without a colon after it");
        assert_eq!(
            error.to_string(),
            "a member's name without a colon after it at line 3, column 7"
        );
        // Columns count characters, not bytes.
        let error = parse("[\"かな\", x]".as_bytes(), DEFAULT_MAX_DEPTH).expect_err("not JSON");
        assert_eq!((error.line, error.column), (1, 8));
        // A name given twice is reported where it is given the second time,
        // the earliest such in the object.
        let error = parse(b"{\"b\":1,\"a\":2,\n\"a\":3,\"b\":4}", DEFAULT_MAX_DEPTH).expect_err("twice");
        assert_eq!(error.message, "a name given twice in one object");
        assert_eq!((error.line, error.column), (2, 1));
        let error = parse(b"\"a\" \xff", DEFAULT_MAX_DEPTH).expect_err("not UTF-8");
        assert_eq!(
            (error.offset, error.column, error.message),
            (4, 5, "bytes that are not UTF-8")
        );
    }

    #[test]
    fn values_are_written_as_they_read() {
        let text = br#"{"a": [1, -0.5e+3, "x"], "b": {"c": null, "d": [[], {}, [true]]}, "e": {}}"#;
        let value = read(text);
        let written = write(&value);
        assert_eq!(
            written,
            "{\n  \"a\": [1, -0.5e+3, \"x\"],\n  \"b\": {\n    \"c\": null,\n    \"d\": [\n      [],\n      {},\n      [true]\n    ]\n  },\n  \"e\": {}\n}\n"
        );
        assert_eq!(read(written.as_bytes()), value);
        let canonical = write_canonical(&value);
        assert_eq!(
            canonical,
            r#"{"a":[1,-500,"x"],"b":{"c":null,"d":[[],{},[true]]},"e":{}}"#
        );
        assert_eq!(
            read(canonical.as_bytes()),
            read(br#"{"a": [1, -500, "x"], "b": {"c": null, "d": [[], {}, [true]]}, "e": {}}"#)
        );
        assert_eq!(write(&Value::Null), "null\n");
        assert_eq!(write_canonical(&read(b"[]")), "[]");
    }

    #[test]
    fn the_canonical_form_orders_members_by_utf16() {
        // U+1F600 is D83D DE00 in UTF-16, before U+E000, which UTF-8 and
        // Unicode's order put first (RFC 8785 §3.2.3).
        let value = read(
            "{\"b\": 1, \"\u{e000}\": 5, \"a\": {\"z\": 1E2, \"y\": -0}, \"\u{1f600}\": 4, \"é\": 3, \"1e400\": 1e400}"
                .as_bytes(),
        );
        assert_eq!(
            write_canonical(&value),
            "{\"1e400\":1e400,\"a\":{\"y\":0,\"z\":100},\"b\":1,\"é\":3,\"\u{1f600}\":4,\"\u{e000}\":5}"
        );
        // The pretty form keeps the order and the numbers' texts.
        assert!(
            write(&value)
                .starts_with("{\n  \"b\": 1,\n  \"\u{e000}\": 5,\n  \"a\": {\n    \"z\": 1E2,\n    \"y\": -0\n  },")
        );
    }

    #[test]
    fn numbers_are_written_as_ecmascript_writes_them() {
        let text = |value: f64| Number::from_f64(value).map(|number| number.as_text().to_owned());
        // RFC 8785's Appendix B, by the binary64s' bits, which V8's
        // JSON.stringify writes the same.
        for (bits, written) in [
            (0x0000_0000_0000_0000, "0"),
            (0x8000_0000_0000_0000, "0"),
            (0x0000_0000_0000_0001, "5e-324"),
            (0x8000_0000_0000_0001, "-5e-324"),
            (0x7fef_ffff_ffff_ffff, "1.7976931348623157e+308"),
            (0xffef_ffff_ffff_ffff, "-1.7976931348623157e+308"),
            (0x4340_0000_0000_0000, "9007199254740992"),
            (0xc340_0000_0000_0000, "-9007199254740992"),
            (0x4430_0000_0000_0000, "295147905179352830000"),
            (0x44b5_2d02_c7e1_4af5, "9.999999999999997e+22"),
            (0x44b5_2d02_c7e1_4af6, "1e+23"),
            (0x44b5_2d02_c7e1_4af7, "1.0000000000000001e+23"),
            (0x444b_1ae4_d6e2_ef4e, "999999999999999700000"),
            (0x444b_1ae4_d6e2_ef4f, "999999999999999900000"),
            (0x444b_1ae4_d6e2_ef50, "1e+21"),
            (0x3eb0_c6f7_a0b5_ed8c, "9.999999999999997e-7"),
            (0x3eb0_c6f7_a0b5_ed8d, "0.000001"),
            (0x41b3_de43_5555_5553, "333333333.3333332"),
            (0x41b3_de43_5555_5554, "333333333.33333325"),
            (0x41b3_de43_5555_5555, "333333333.3333333"),
            (0x41b3_de43_5555_5556, "333333333.3333334"),
            (0x41b3_de43_5555_5557, "333333333.33333343"),
            (0xbecb_f647_612f_3696, "-0.0000033333333333333333"),
            // Halfway between …206.2 and …206.3, which Rust writes …206.3.
            (0x4314_3ff3_c1cb_0959, "1424953923781206.2"),
            // Halfway as well, among the 34 889 of 1 199 867 binary64s that
            // were so, every one written as V8 writes it.
            (0x4312_9879_088f_f039, "1308548795726862.2"),
            (0xc316_fed6_ebaf_054d, "-1618162129551699.2"),
            (0xc316_6196_c414_4b69, "-1574937412571866.2"),
        ] {
            assert_eq!(text(f64::from_bits(bits)).as_deref(), Some(written), "{bits:#018x}");
        }
        for (value, written) in [
            (99.0, "99"),
            (80.5, "80.5"),
            (0.1, "0.1"),
            (1e300, "1e+300"),
            (2.0_f64.powi(-53), "1.1102230246251565e-16"),
            (1.5 / 49096.0, "0.00003055238715985009"),
            (0.001, "0.001"),
            (1e-7, "1e-7"),
            (1e20, "100000000000000000000"),
            (1.234_567_890_123_456_8e20, "123456789012345680000"),
            (-1.23456e-76, "-1.23456e-76"),
            (123e-20, "1.23e-18"),
            (1.0 / 1024.0, "0.0009765625"),
        ] {
            assert_eq!(text(value).as_deref(), Some(written));
            assert_eq!(
                read(written.as_bytes()).as_f64().map(f64::to_bits),
                Some(value.to_bits())
            );
        }
        assert_eq!(text(f64::NAN), None);
        assert_eq!(text(f64::INFINITY), None);
        assert_eq!(text(f64::NEG_INFINITY), None);
        assert_eq!(Number::from_f64(-0.0).map(|number| number.as_f64().to_bits()), Some(0));
        let number = Number::from_u64(u64::MAX);
        assert_eq!(
            (number.as_text(), number.as_u64()),
            ("18446744073709551615", Some(u64::MAX))
        );
    }

    #[test]
    fn strings_are_written_with_their_escapes() {
        let string = "a\"\\/\u{8}\u{c}\n\r\t\u{1}\u{1f}\u{7f}é\u{1f600}";
        let written = write_canonical(&Value::String(string.to_owned()));
        assert_eq!(written, "\"a\\\"\\\\/\\b\\f\\n\\r\\t\\u0001\\u001f\u{7f}é\u{1f600}\"");
        assert_eq!(read(written.as_bytes()).as_str(), Some(string));
        assert_eq!(write(&Value::String(string.to_owned())), format!("{written}\n"));
    }

    #[test]
    fn objects_keep_where_they_and_their_names_are() {
        let text = b"{\n  \"a\": 1,\n  \"\xe3\x81\x8b\": {\"b\": 2},\n  \"c\": [{}]\n}";
        let value = read(text);
        let object = value.as_object().expect("an object");
        assert_eq!((object.start(), object.offset("a")), (0, Some(4)));
        let offset = object.offset("\u{304b}").expect("a member");
        assert_eq!(line_and_column(text, offset), (3, 3));
        let inner = object.get("\u{304b}").and_then(Value::as_object).expect("an object");
        assert_eq!(line_and_column(text, inner.start()), (3, 8));
        assert_eq!(
            inner.offset("b").map(|offset| line_and_column(text, offset)),
            Some((3, 9))
        );
        let empty = object
            .get("c")
            .and_then(Value::as_array)
            .and_then(|c| c[0].as_object())
            .expect("an object");
        assert_eq!(line_and_column(text, empty.start()), (4, 9));
        assert_eq!(object.offset("d"), None);
        let offsets: Vec<usize> = object.iter_offsets().map(|(_, _, offset)| offset).collect();
        assert_eq!(offsets, [4, 14, 33]);
        // Equal members are equal objects, wherever they were read from.
        let mut built = Object::new();
        built.push("b", read(b"2"));
        assert_eq!(built.start(), 0);
        assert_eq!(&Value::Object(built), object.get("\u{304b}").expect("a member"));
    }

    #[test]
    fn nesting_is_limited_and_takes_no_stack() {
        let deep = |depth: usize| [vec![b'['; depth], vec![b']'; depth]].concat();
        assert!(parse(&deep(DEFAULT_MAX_DEPTH), DEFAULT_MAX_DEPTH).is_ok());
        let error = parse(&deep(DEFAULT_MAX_DEPTH + 1), DEFAULT_MAX_DEPTH).expect_err("too deep");
        assert_eq!(error.message, "arrays and objects nested deeper than json.max_depth");
        assert_eq!(error.offset, DEFAULT_MAX_DEPTH);
        assert!(parse(b"[]", 0).is_err());
        assert!(parse(b"1", 0).is_ok());
        // A million levels, read and dropped on the stack of a test's thread.
        let depth = 1_000_000;
        let value = parse(&deep(depth), depth).expect("deep JSON");
        let mut level = &value;
        let mut levels = 0;
        while let Some([inner]) = level.as_array() {
            level = inner;
            levels += 1;
        }
        assert_eq!(levels, depth - 1);
        // Written canonically, in a text as long as the one read. Written in
        // lines, each indented as deep as it lies, it would take some 2 n²
        // bytes, 2 TB here: the lines are tried at a thousand levels, 2 MB.
        assert_eq!(write_canonical(&value).len(), 2 * depth);
        drop(value);
        assert_eq!(write(&read(b"[[[]]]")), "[\n  [[]]\n]\n");
        let thousand = parse(&deep(1000), 1000).expect("deep JSON");
        let lines = write(&thousand);
        assert!(lines.len() < 2 * 1000 * 1000 + 4 * 1000);
        assert_eq!(parse(lines.as_bytes(), 1000).as_ref(), Ok(&thousand));
        let objects = [b"{\"a\":".repeat(depth), b"null".to_vec(), b"}".repeat(depth)].concat();
        drop(parse(&objects, depth).expect("deep JSON"));
    }
}
