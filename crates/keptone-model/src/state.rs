// SPDX-FileCopyrightText: 2026 Yuma Kakei <yumasansansan@gmail.com>
// SPDX-License-Identifier: GPL-3.0-or-later
//
// This file is part of Keptone. The SPDX lines name its copyright holder and its
// license, in the machine-readable form of the REUSE specification: the GNU
// General Public License, version 3 or any later version
// (LICENSES/GPL-3.0-or-later.txt).

//! The native state (docs/STATE-FORMAT.md): what Keptone saves of itself and
//! reads back, a text in the subset of JSON that [`crate::json`] reads.
//!
//! A [`State`] holds every setting, the Scala files of the tuning, and the
//! parts, as many as `part.count` says, each with its patch. [`write`](fn@write) writes
//! one as text, and [`read`] makes one of a text, or refuses the text whole
//! and says where in it and why (docs/DESIGN.md §11.1). A state holds each
//! value exactly, every number a binary64 written as the shortest text that
//! reads back as it, and a reader takes nothing it cannot hold so: a member
//! that the state's version has and the text lacks, a value of another kind
//! than its member's or one that its member does not take, as a number that
//! is not finite, is refused, the text's line and column given. A number is
//! taken beyond its instrument's scale as well (docs/MATH.md §0.5). What a later version adds, which this one does not know, is
//! kept as it was read and written back after what it knows, at every level,
//! so that a state passes through an older Keptone whole.
//!
//! Each part's patch carries the SHA-256 of its members, written in the
//! canonical form of RFC 8785 \[RFC8785\]: those of the voice, the supplement
//! and the fractional scaling, a later version's among them. A digest that is
//! not theirs is reported as a [`Finding`], the state is read all the same,
//! and it is written with theirs.

use std::collections::HashSet;
use std::fmt;

use crate::dx7ii::{FractionalScaling, Offset};
use crate::id::{Area, Kind, PartParameter};
use crate::json::{self, Number, Object, Value};
use crate::part::Part;
use crate::scala::{self, Mapping, Scale};
use crate::settings::{self, SPECS, Settings, Spec};
use crate::sha256;
use crate::supplement::Supplement;
use crate::voice::{self, Name};

/// The format's name, which a state gives as its member `format`.
pub const FORMAT: &str = "keptone-state";

/// The version of the format that this build writes, the first: what members
/// a state of it holds. A later version adds members and changes none.
pub const VERSION: u64 = 1;

/// What a setting that is a number or the default that MATH gives as a
/// formula takes for the default.
const AUTOMATIC: &str = "auto";

/// The member of a part that holds its patch's digest.
const DIGEST: &str = "patch.sha256";

/// The member of a part that holds its voice's name.
const NAME: &str = "voice.name";

/// How the names of a part's members that belong to its patch begin, in this
/// version and in those to come: every such member is in the patch's digest,
/// but the digest itself.
const OF_PATCH: [&str; 4] = ["voice.", "supplement.", "fractional_scaling.", "patch."];

/// How many groups of keys a fractional scaling gives a level for.
const GROUPS: usize = 40;

/// The members of a state that this build knows, in the order it writes them.
const STATE_MEMBERS: [&str; 5] = ["format", "version", "settings", "tuning", "parts"];

/// The members of the tuning that this build knows, in the order it writes
/// them.
const TUNING_MEMBERS: [&str; 2] = ["scale", "keyboard_mapping"];

/// A Scala file of the tuning, kept as its text beside what it says.
#[derive(Debug, Clone, PartialEq)]
pub struct ScalaFile<T> {
    /// The file's text, as it was given.
    text: String,
    /// What it says.
    value: T,
}

impl<T> ScalaFile<T> {
    /// The file's text.
    #[must_use]
    pub fn text(&self) -> &str {
        &self.text
    }

    /// What it says.
    #[must_use]
    pub const fn value(&self) -> &T {
        &self.value
    }
}

impl ScalaFile<Scale> {
    /// The scale of a `.scl` file's text.
    ///
    /// # Errors
    ///
    /// Where the text is not a scale ([`scala::parse_scl`]).
    pub fn scale(text: String) -> Result<Self, scala::Error> {
        scala::parse_scl(text.as_bytes()).map(|value| Self { text, value })
    }
}

impl ScalaFile<Mapping> {
    /// The keyboard mapping of a `.kbm` file's text.
    ///
    /// # Errors
    ///
    /// Where the text is not a keyboard mapping ([`scala::parse_kbm`]).
    pub fn keyboard_mapping(text: String) -> Result<Self, scala::Error> {
        scala::parse_kbm(text.as_bytes()).map(|value| Self { text, value })
    }
}

/// The Scala files that the tuning takes where `tuning.source` is `scala`
/// (docs/MATH.md §2.1), each if there is one.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Tuning {
    /// The scale.
    pub scale: Option<ScalaFile<Scale>>,
    /// The keyboard mapping.
    pub keyboard_mapping: Option<ScalaFile<Mapping>>,
}

/// A state: every setting, the tuning's Scala files and the parts, as many as
/// `part.count` says, and the members of a later version's it was read with.
#[derive(Debug, Clone, PartialEq)]
pub struct State {
    /// Every setting.
    settings: Settings,
    /// The tuning's Scala files.
    pub tuning: Tuning,
    /// The parts.
    parts: Vec<Part>,
    /// The members it was read with that this build does not know.
    kept: Kept,
}

/// The members of a state's objects that this build does not know, each
/// object's in the order they were read, to be written after the members it
/// knows.
#[derive(Debug, Clone, PartialEq, Default)]
struct Kept {
    /// The state's own.
    state: Object,
    /// The settings'.
    settings: Object,
    /// The tuning's.
    tuning: Object,
    /// Each part's, one for each part.
    parts: Vec<Object>,
}

impl State {
    /// The state that Keptone starts in: every setting at its default, no
    /// Scala file, and as many parts as `part.count` says, as they start, the
    /// first on and the others off (docs/MATH.md §13.5).
    #[must_use]
    pub fn new() -> Self {
        let settings = Settings::new();
        let count = part_count(&settings);
        Self {
            settings,
            tuning: Tuning::default(),
            parts: (0..count).map(|index| Part::initial(index == 0)).collect(),
            kept: Kept {
                parts: vec![Object::new(); count],
                ..Kept::default()
            },
        }
    }

    /// The settings.
    #[must_use]
    pub const fn settings(&self) -> &Settings {
        &self.settings
    }

    /// Sets the setting of a number to a value, as [`Settings::set`] does,
    /// and says whether it did. Setting `part.count` adds parts, off as they
    /// start, or takes the last away with what they were read with, so that
    /// there are as many as it says; a count of more parts than memory can be
    /// had for is not taken, and leaves the state as it was.
    pub fn set_setting(&mut self, number: u16, value: settings::Value) -> bool {
        let before = self.settings.clone();
        if !self.settings.set(number, value) {
            return false;
        }
        let count = part_count(&self.settings);
        let more = count.saturating_sub(self.parts.len());
        if self.parts.try_reserve_exact(more).is_err() || self.kept.parts.try_reserve_exact(more).is_err() {
            self.settings = before;
            return false;
        }
        self.parts.truncate(count);
        while self.parts.len() < count {
            self.parts.push(Part::initial(false));
        }
        self.kept.parts.resize_with(count, Object::new);
        true
    }

    /// The parts, the first first.
    #[must_use]
    pub fn parts(&self) -> &[Part] {
        &self.parts
    }

    /// The parts, to change; how many there are, `part.count` says.
    pub fn parts_mut(&mut self) -> &mut [Part] {
        &mut self.parts
    }
}

impl Default for State {
    fn default() -> Self {
        Self::new()
    }
}

/// What reading a state found that it read all the same.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Finding {
    /// A part's patch gave a digest that its members do not have. The state
    /// is written with the one they have.
    Digest {
        /// The part's index, 0 for the first.
        part: usize,
        /// What the part gave.
        found: String,
        /// The digest its patch's members have.
        computed: String,
    },
}

/// Why a text is not a state, and where.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Error {
    /// The offset in the text of what is wrong: of the name of the member
    /// whose value is wrong, of the opening brace of the object that lacks a
    /// member, or where the text stopped being JSON.
    pub offset: usize,
    /// Its line, counted from 1, as [`json::Error`] counts it.
    pub line: usize,
    /// Its column, counted from 1 in characters.
    pub column: usize,
    /// The place in the state of what is wrong, as a JSON Pointer
    /// \[RFC6901\], as `/parts/0/voice.op6.eg.rate.1`; empty for the text
    /// itself, where it is not JSON.
    pub pointer: String,
    /// What is wrong.
    pub message: String,
}

impl fmt::Display for Error {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.pointer.is_empty() {
            write!(
                formatter,
                "{} at line {}, column {}",
                self.message, self.line, self.column
            )
        } else {
            write!(
                formatter,
                "{} at {}, line {}, column {}",
                self.message, self.pointer, self.line, self.column
            )
        }
    }
}

impl std::error::Error for Error {}

/// The state of a text, and what was found in it that it was read with all
/// the same; the text's arrays and objects may nest `max_depth` deep at the
/// most (`json.max_depth`, docs/MATH.md §17), and a state nests four deep.
///
/// # Errors
///
/// Where the text is not JSON, or not a state that this build can hold
/// exactly: where it is not of this format or of a version; lacks a member
/// that its version has, a member of a supplement or a fractional scaling
/// among them where it has another; holds a value of another kind than its
/// member's or one that its member does not take, as a number that is not
/// finite, or a Scala file that does not read; or holds
/// another count of parts than `part.count` says.
pub fn read(bytes: &[u8], max_depth: usize) -> Result<(State, Vec<Finding>), Error> {
    let value = json::parse(bytes, max_depth).map_err(|error| Error {
        offset: error.offset,
        line: error.line,
        column: error.column,
        pointer: String::new(),
        message: error.message.to_owned(),
    })?;
    let reader = Reader { bytes };
    let Some(state) = value.as_object() else {
        let start = bytes.iter().position(|byte| !b" \t\n\r".contains(byte)).unwrap_or(0);
        return Err(reader.error(start, String::new(), "a text that is not a JSON object"));
    };
    let format = reader.member(state, "", "format")?;
    if format.value.as_str() != Some(FORMAT) {
        return Err(reader.refuse(&format, "a format other than keptone-state"));
    }
    let version = reader.member(state, "", "version")?;
    if version.value.as_u64().is_none_or(|version| version == 0) {
        return Err(reader.refuse(&version, "a version that is not an integer above 0"));
    }
    let (settings, kept_settings) = read_settings(&reader, &reader.member(state, "", "settings")?)?;
    let (tuning, kept_tuning) = read_tuning(&reader, &reader.member(state, "", "tuning")?)?;
    let names = Names::new();
    let parts = read_parts(
        &reader,
        &reader.member(state, "", "parts")?,
        part_count(&settings),
        &names,
    )?;
    let kept = Kept {
        state: kept_of(state, |name| STATE_MEMBERS.contains(&name)),
        settings: kept_settings,
        tuning: kept_tuning,
        parts: parts.kept,
    };
    Ok((
        State {
            settings,
            tuning,
            parts: parts.parts,
            kept,
        },
        parts.findings,
    ))
}

/// A state as text (docs/STATE-FORMAT.md): an object of its format, its
/// version, its settings, its tuning and its parts, each object's members as
/// this build knows them and then those of a later version's, a member to a
/// line; every number as [`json::Number::from_f64`] writes it, and each part's
/// patch with the digest of its members.
#[must_use]
pub fn write(state: &State) -> String {
    let names = Names::new();
    let mut settings = Object::new();
    for (number, spec) in (0_u16..).zip(SPECS.iter()) {
        if let Some(value) = state.settings.get(number) {
            settings.push(spec.name, setting_value(spec, value));
        }
    }
    extend(&mut settings, &state.kept.settings);
    let mut tuning = Object::new();
    let text = |text: &str| Value::String(text.to_owned());
    tuning.push(
        TUNING_MEMBERS[0],
        state
            .tuning
            .scale
            .as_ref()
            .map_or(Value::Null, |file| text(file.text())),
    );
    tuning.push(
        TUNING_MEMBERS[1],
        state
            .tuning
            .keyboard_mapping
            .as_ref()
            .map_or(Value::Null, |file| text(file.text())),
    );
    extend(&mut tuning, &state.kept.tuning);
    let parts = state
        .parts
        .iter()
        .zip(&state.kept.parts)
        .map(|(part, kept)| Value::Object(part_object(part, kept, &names)))
        .collect();
    let mut object = Object::new();
    object.push(STATE_MEMBERS[0], text(FORMAT));
    object.push(STATE_MEMBERS[1], Value::Number(Number::from_u64(VERSION)));
    object.push(STATE_MEMBERS[2], Value::Object(settings));
    object.push(STATE_MEMBERS[3], Value::Object(tuning));
    object.push(STATE_MEMBERS[4], Value::Array(parts));
    extend(&mut object, &state.kept.state);
    json::write(&Value::Object(object))
}

/// The names of a part's members that this build knows.
struct Names {
    /// Each parameter's, in the order of their IDs.
    parameters: Vec<(PartParameter, String)>,
    /// The members of each operator's fractional scaling, its offset and its
    /// levels, OP1's first.
    scalings: [(String, String); 6],
    /// Every one of them, the voice's name and the patch's digest as well.
    known: HashSet<String>,
}

impl Names {
    /// The names.
    fn new() -> Self {
        let parameters: Vec<(PartParameter, String)> = PartParameter::all()
            .map(|parameter| (parameter, parameter.name()))
            .collect();
        let scalings: [(String, String); 6] = std::array::from_fn(|place| {
            let operator = format!("fractional_scaling.op{}", place + 1);
            (format!("{operator}.offset"), format!("{operator}.levels"))
        });
        let known = parameters
            .iter()
            .map(|(_, name)| name.clone())
            .chain(
                scalings
                    .iter()
                    .flat_map(|(offset, levels)| [offset.clone(), levels.clone()]),
            )
            .chain([NAME.to_owned(), DIGEST.to_owned()])
            .collect();
        Self {
            parameters,
            scalings,
            known,
        }
    }

    /// The parameters of an area and their names, in the order of their IDs.
    fn of(&self, area: Area) -> impl Iterator<Item = &(PartParameter, String)> {
        self.parameters
            .iter()
            .filter(move |(parameter, _)| parameter.area() == area)
    }
}

/// What reads a state's text: the text, for the lines and columns of what it
/// refuses.
struct Reader<'a> {
    /// The text.
    bytes: &'a [u8],
}

/// A member that a reader has found: its value, where its name is in the
/// text, and its place in the state as a JSON Pointer.
struct Member<'a> {
    /// Its value.
    value: &'a Value,
    /// The offset of its name.
    offset: usize,
    /// Its place in the state.
    pointer: String,
}

impl Reader<'_> {
    /// The error of what is wrong at an offset of the text and a place in the
    /// state.
    fn error(&self, offset: usize, pointer: String, message: impl Into<String>) -> Error {
        let (line, column) = json::line_and_column(self.bytes, offset);
        Error {
            offset,
            line,
            column,
            pointer,
            message: message.into(),
        }
    }

    /// The error of a member whose value is wrong.
    fn refuse(&self, member: &Member<'_>, message: impl Into<String>) -> Error {
        self.error(member.offset, member.pointer.clone(), message)
    }

    /// A member of an object at a place in the state, which it must have.
    fn member<'v>(&self, object: &'v Object, at: &str, name: &str) -> Result<Member<'v>, Error> {
        let pointer = pointer(at, name);
        match (object.get(name), object.offset(name)) {
            (Some(value), Some(offset)) => Ok(Member { value, offset, pointer }),
            _ => Err(self.error(object.start(), pointer, "a member that the state lacks")),
        }
    }

    /// A member's value, which must be an object.
    fn object<'v>(&self, member: &Member<'v>) -> Result<&'v Object, Error> {
        member
            .value
            .as_object()
            .ok_or_else(|| self.refuse(member, "a value that is not an object"))
    }
}

/// The settings of their member, and what of the member's it does not know.
fn read_settings(reader: &Reader<'_>, member: &Member<'_>) -> Result<(Settings, Object), Error> {
    let object = reader.object(member)?;
    let mut settings = Settings::new();
    for (number, spec) in (0_u16..).zip(SPECS.iter()) {
        let setting = reader.member(object, &member.pointer, spec.name)?;
        let taken = setting_of(spec, setting.value).is_some_and(|value| settings.set(number, value));
        if !taken {
            return Err(reader.refuse(&setting, "a value that the setting does not take"));
        }
    }
    let kept = kept_of(object, |name| settings::number_of(name).is_some());
    Ok((settings, kept))
}

/// The tuning of its member, and what of the member's it does not know.
fn read_tuning(reader: &Reader<'_>, member: &Member<'_>) -> Result<(Tuning, Object), Error> {
    let object = reader.object(member)?;
    let scale = reader.member(object, &member.pointer, TUNING_MEMBERS[0])?;
    let keyboard_mapping = reader.member(object, &member.pointer, TUNING_MEMBERS[1])?;
    let tuning = Tuning {
        scale: scala_file(reader, &scale, ScalaFile::scale)?,
        keyboard_mapping: scala_file(reader, &keyboard_mapping, ScalaFile::keyboard_mapping)?,
    };
    Ok((tuning, kept_of(object, |name| TUNING_MEMBERS.contains(&name))))
}

/// The Scala file of a member, its text read as `read` reads it, or none
/// where the member is null.
fn scala_file<T>(
    reader: &Reader<'_>,
    member: &Member<'_>,
    read: fn(String) -> Result<ScalaFile<T>, scala::Error>,
) -> Result<Option<ScalaFile<T>>, Error> {
    match member.value {
        Value::Null => Ok(None),
        Value::String(text) => read(text.clone()).map(Some).map_err(|error| {
            reader.refuse(
                member,
                format!("{}, at the Scala file's line {}", error.message, error.line),
            )
        }),
        _ => Err(reader.refuse(member, "a value that is neither a string nor null")),
    }
}

/// What the parts of a state read as: the parts, what of each part's members
/// this build does not know, and what was found.
struct PartsRead {
    /// The parts.
    parts: Vec<Part>,
    /// What of each part's members this build does not know.
    kept: Vec<Object>,
    /// What was found.
    findings: Vec<Finding>,
}

/// The parts of their member, as many as `count`.
fn read_parts(reader: &Reader<'_>, member: &Member<'_>, count: usize, names: &Names) -> Result<PartsRead, Error> {
    let values = member
        .value
        .as_array()
        .ok_or_else(|| reader.refuse(member, "a value that is not an array"))?;
    if values.len() != count {
        return Err(reader.refuse(member, "a count of parts other than part.count"));
    }
    let mut read = PartsRead {
        parts: Vec::with_capacity(count),
        kept: Vec::with_capacity(count),
        findings: Vec::new(),
    };
    for (index, value) in values.iter().enumerate() {
        let at = format!("{}/{index}", member.pointer);
        let object = value
            .as_object()
            .ok_or_else(|| reader.error(member.offset, at.clone(), "a part that is not an object"))?;
        let (part, digest) = read_part(reader, object, &at, names)?;
        if let Some((found, computed)) = digest {
            read.findings.push(Finding::Digest {
                part: index,
                found,
                computed,
            });
        }
        read.parts.push(part);
        read.kept.push(kept_of(object, |name| names.known.contains(name)));
    }
    Ok(read)
}

/// A part of its object at a place in the state, and the digest it gave and
/// the one its patch's members have, where the two differ.
fn read_part(
    reader: &Reader<'_>,
    object: &Object,
    at: &str,
    names: &Names,
) -> Result<(Part, Option<(String, String)>), Error> {
    let mut part = Part::initial(false);
    for area in [Area::Part, Area::Function, Area::Voice] {
        for (parameter, name) in names.of(area) {
            read_parameter(reader, object, at, &mut part, *parameter, name)?;
        }
    }
    let name = reader.member(object, at, NAME)?;
    part.patch.voice.name = name
        .value
        .as_str()
        .and_then(name_of)
        .ok_or_else(|| reader.refuse(&name, "a name that is not ten characters, each below U+0080"))?;
    // A supplement or a fractional scaling is there where any of its members
    // is, and then each of them must be.
    if names.of(Area::Supplement).any(|(_, name)| object.get(name).is_some()) {
        part.patch.supplement = Some(Supplement::zero());
        for (parameter, name) in names.of(Area::Supplement) {
            read_parameter(reader, object, at, &mut part, *parameter, name)?;
        }
    }
    let scaling = names
        .scalings
        .iter()
        .any(|(offset, levels)| object.get(offset).is_some() || object.get(levels).is_some());
    if scaling {
        part.patch.fractional_scaling = Some(read_scaling(reader, object, at, names)?);
    }
    let digest = reader.member(object, at, DIGEST)?;
    let found = digest
        .value
        .as_str()
        .ok_or_else(|| reader.refuse(&digest, "a digest that is not a string"))?;
    let computed = digest_of(object.iter());
    Ok((part, (found != computed).then(|| (found.to_owned(), computed))))
}

/// Sets a parameter of a part to its member's value.
fn read_parameter(
    reader: &Reader<'_>,
    object: &Object,
    at: &str,
    part: &mut Part,
    parameter: PartParameter,
    name: &str,
) -> Result<(), Error> {
    let member = reader.member(object, at, name)?;
    let value = parameter_of(parameter.kind(), member.value).map_err(|message| reader.refuse(&member, message))?;
    if part.set_value(parameter, value) {
        Ok(())
    } else {
        Err(reader.refuse(&member, "a number that the parameter does not take"))
    }
}

/// The fractional scaling of a part's members.
fn read_scaling(reader: &Reader<'_>, object: &Object, at: &str, names: &Names) -> Result<FractionalScaling, Error> {
    let mut scaling = FractionalScaling::zero();
    for (operator, (offset_name, levels_name)) in scaling.operators.iter_mut().zip(&names.scalings) {
        let offset = reader.member(object, at, offset_name)?;
        operator.offset = offset
            .value
            .as_f64()
            .and_then(Offset::new)
            .ok_or_else(|| reader.refuse(&offset, "an offset that is not a finite number"))?;
        let levels = reader.member(object, at, levels_name)?;
        let values = levels
            .value
            .as_array()
            .filter(|values| values.len() == GROUPS)
            .ok_or_else(|| reader.refuse(&levels, "a value that is not an array of 40 levels"))?;
        for ((level, value), group) in operator.levels.iter_mut().zip(values).zip(0_usize..) {
            *level = value.as_f64().and_then(voice::Value::new).ok_or_else(|| {
                reader.error(
                    levels.offset,
                    format!("{}/{group}", levels.pointer),
                    "a level that is not a finite number",
                )
            })?;
        }
    }
    Ok(scaling)
}

/// The members of an object whose names `known` does not know, in their
/// order.
fn kept_of(object: &Object, known: impl Fn(&str) -> bool) -> Object {
    let mut kept = Object::new();
    for (name, value) in object.iter().filter(|(name, _)| !known(name)) {
        kept.push(name, value.clone());
    }
    kept
}

/// Adds the members of `kept` after an object's.
fn extend(object: &mut Object, kept: &Object) {
    for (name, value) in kept.iter() {
        object.push(name, value.clone());
    }
}

/// A part as an object: what it has of its own, its function data, its
/// patch's voice, the voice's name, its supplement and its fractional
/// scaling where it has them, the patch's digest, and then what of a later
/// version's it was read with.
fn part_object(part: &Part, kept: &Object, names: &Names) -> Object {
    let mut object = Object::new();
    for area in [Area::Part, Area::Function, Area::Voice, Area::Supplement] {
        for (parameter, name) in names.of(area) {
            // A supplement's parameters have values where the patch has one.
            if let Some(value) = part.value(*parameter) {
                object.push(name.clone(), parameter_value(parameter.kind(), value));
            }
        }
        if area == Area::Voice {
            let name = part
                .patch
                .voice
                .name
                .bytes()
                .iter()
                .map(|&byte| char::from(byte))
                .collect();
            object.push(NAME, Value::String(name));
        }
    }
    if let Some(scaling) = &part.patch.fractional_scaling {
        for (operator, (offset, levels)) in scaling.operators.iter().zip(&names.scalings) {
            object.push(offset.clone(), number(operator.offset.get()));
            let levels_value = operator.levels.iter().map(|level| number(level.get())).collect();
            object.push(levels.clone(), Value::Array(levels_value));
        }
    }
    let digest = digest_of(object.iter().chain(kept.iter()));
    object.push(DIGEST, Value::String(digest));
    extend(&mut object, kept);
    object
}

/// The digest of a patch whose part has these members: the SHA-256, in
/// lowercase hexadecimal, of an object of the members that belong to the
/// patch, written in RFC 8785's canonical form ([`json::write_canonical`]).
fn digest_of<'a>(members: impl Iterator<Item = (&'a str, &'a Value)>) -> String {
    let mut patch = Object::new();
    for (name, value) in members.filter(|(name, _)| of_patch(name)) {
        patch.push(name, value.clone());
    }
    let canonical = json::write_canonical(&Value::Object(patch));
    sha256::hex(&sha256::digest(canonical.as_bytes()))
}

/// Whether a member of a part belongs to its patch, and so to its digest.
fn of_patch(name: &str) -> bool {
    name != DIGEST && OF_PATCH.iter().any(|beginning| name.starts_with(beginning))
}

/// A parameter's value as JSON: a choice's name, a switch's boolean, or a
/// number.
fn parameter_value(kind: Kind, value: f64) -> Value {
    match kind {
        Kind::Choice(names) => voice::step_of(value, u8::try_from(names.len()).unwrap_or(u8::MAX))
            .and_then(|step| names.get(usize::from(step)))
            .map_or(Value::Null, |name| Value::String((*name).to_owned())),
        Kind::Switch => Value::Bool(voice::switch_of(value).unwrap_or(false)),
        Kind::Number(_) | Kind::Signed(_) | Kind::Integer(..) | Kind::Count => number(value),
    }
}

/// A parameter's value of JSON, as [`parameter_value`] writes it, or what is
/// wrong with it.
fn parameter_of(kind: Kind, value: &Value) -> Result<f64, &'static str> {
    match kind {
        Kind::Choice(names) => value
            .as_str()
            .and_then(|name| names.iter().position(|known| *known == name))
            .and_then(|index| u8::try_from(index).ok())
            .map(f64::from)
            .ok_or("a value that is none of the parameter's names"),
        Kind::Switch => value
            .as_bool()
            .map(|on| f64::from(u8::from(on)))
            .ok_or("a value that is neither true nor false"),
        Kind::Number(_) | Kind::Signed(_) | Kind::Integer(..) | Kind::Count => {
            value.as_f64().ok_or("a value that is not a number")
        }
    }
}

/// A setting's value as JSON: a switch's boolean, a choice's name, a number,
/// `auto` for the default that MATH gives as a formula, or an integer, written
/// whole.
fn setting_value(spec: &Spec, value: settings::Value) -> Value {
    match (spec.kind, value) {
        (_, settings::Value::Switch(on)) => Value::Bool(on),
        (settings::Kind::Choice(names), settings::Value::Choice(index)) => names
            .get(usize::from(index))
            .map_or(Value::Null, |name| Value::String((*name).to_owned())),
        (_, settings::Value::Number(value)) => number(value),
        (_, settings::Value::Automatic) => Value::String(AUTOMATIC.to_owned()),
        (_, settings::Value::Integer(integer)) => Value::Number(Number::from_u64(integer)),
        (_, settings::Value::Choice(_)) => Value::Null,
    }
}

/// A setting's value of JSON, as [`setting_value`] writes it, if it is one of
/// the setting's kind; whether the setting takes it, [`Settings::set`] says.
fn setting_of(spec: &Spec, value: &Value) -> Option<settings::Value> {
    match spec.kind {
        settings::Kind::Switch => value.as_bool().map(settings::Value::Switch),
        settings::Kind::Choice(names) => value
            .as_str()
            .and_then(|name| names.iter().position(|known| *known == name))
            .and_then(|index| u8::try_from(index).ok())
            .map(settings::Value::Choice),
        settings::Kind::NumberOrAutomatic(_) if value.as_str() == Some(AUTOMATIC) => Some(settings::Value::Automatic),
        settings::Kind::Number(_) | settings::Kind::NumberOrAutomatic(_) => value.as_f64().map(settings::Value::Number),
        settings::Kind::Count(_) | settings::Kind::PowerOfTwo | settings::Kind::Seed => {
            value.as_u64().map(settings::Value::Integer)
        }
    }
}

/// A finite binary64 as a JSON number, written as [`Number::from_f64`] writes
/// it.
fn number(value: f64) -> Value {
    Number::from_f64(value).map_or(Value::Null, Value::Number)
}

/// How many parts the settings say there are: `part.count`.
fn part_count(settings: &Settings) -> usize {
    match settings::number_of("part.count").and_then(|number| settings.get(number)) {
        Some(settings::Value::Integer(count)) => usize::try_from(count).unwrap_or(usize::MAX),
        _ => 1,
    }
}

/// The voice's name that a string is, if it is ten characters, each below
/// U+0080: the name's bytes, as [`part_object`] writes them.
fn name_of(text: &str) -> Option<Name> {
    let bytes: Vec<u8> = text
        .chars()
        .map(|character| u8::try_from(character).ok())
        .collect::<Option<_>>()?;
    Name::new(bytes.try_into().ok()?)
}

/// The JSON Pointer of a member of the value at a pointer \[RFC6901\]. No
/// member that a reader looks for has a `~` or a `/` in its name, which the
/// pointer would write `~0` and `~1`.
fn pointer(at: &str, name: &str) -> String {
    format!("{at}/{name}")
}
