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
//! whole, by sorting, not each against every earlier one.

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
}

/// An object: its members in their order, each name once.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Object(Vec<(String, Value)>);

impl Object {
    /// The value of a member.
    #[must_use]
    pub fn get(&self, name: &str) -> Option<&Value> {
        self.0.iter().find(|(member, _)| member == name).map(|(_, value)| value)
    }

    /// The members, in their order.
    pub fn iter(&self) -> impl Iterator<Item = (&str, &Value)> {
        self.0.iter().map(|(name, value)| (name.as_str(), value))
    }

    /// The number of members.
    #[must_use]
    pub fn len(&self) -> usize {
        self.0.len()
    }

    /// Whether the object has no members.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
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
            Self::Object(Object(members)) => within.extend(members.drain(..).map(|(_, value)| value)),
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
                    Value::Object(Object::default())
                } else {
                    let (name, offset) = reader.name()?;
                    open.push(Open::Object(OpenObject {
                        members: Vec::new(),
                        offsets: Vec::new(),
                        name,
                        offset,
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
                            value = Value::Object(Object(object.members));
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

#[cfg(test)]
mod tests {
    use super::{DEFAULT_MAX_DEPTH, Number, Value, parse};

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
        drop(value);
        let objects = [b"{\"a\":".repeat(depth), b"null".to_vec(), b"}".repeat(depth)].concat();
        drop(parse(&objects, depth).expect("deep JSON"));
    }
}
