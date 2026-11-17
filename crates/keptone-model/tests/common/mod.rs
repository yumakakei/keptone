// SPDX-FileCopyrightText: 2026 Yuma Kakei <yumasansansan@gmail.com>
// SPDX-License-Identifier: GPL-3.0-or-later
//
// This file is part of Keptone. The SPDX lines name its copyright holder and its
// license, in the machine-readable form of the REUSE specification: the GNU
// General Public License, version 3 or any later version
// (LICENSES/GPL-3.0-or-later.txt).

//! What the tests of keptone-model share: a random source, and DX7 dumps made
//! by the tests themselves, from \[DEXED-SYSEX\]'s layout stated again here,
//! not from the crate's.

#![allow(dead_code, reason = "each test file takes what it needs of this module")]

/// `SplitMix64`, the generator kiral's tests use: a 64-bit state advanced by a
/// constant and mixed, so that a seed gives the same sequence on every system.
#[derive(Debug, Clone)]
pub struct SplitMix64(pub u64);

impl SplitMix64 {
    /// The next 64 bits.
    pub fn next_u64(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9e37_79b9_7f4a_7c15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
        z ^ (z >> 31)
    }

    /// A uniform integer of [0, n), n not 0.
    pub fn below(&mut self, n: usize) -> usize {
        let n = u64::try_from(n).expect("a usize of 64 bits at most");
        usize::try_from(self.next_u64() % n).expect("below a usize")
    }

    /// A uniform integer of [0, largest].
    pub fn up_to(&mut self, largest: u8) -> u8 {
        u8::try_from(self.next_u64() % (u64::from(largest) + 1)).expect("below 256")
    }

    /// A uniform byte.
    pub fn byte(&mut self) -> u8 {
        self.next_u64().to_le_bytes()[0]
    }
}

/// The largest step of each of an operator's 21 parameters, in the order of
/// the single-voice dump \[DEXED-SYSEX\]: R1 to R4, L1 to L4, the break point
/// and the left and right depths 0–99; the left and right curves 0–3; the rate
/// scaling 0–7; the amplitude sensitivity 0–3; the velocity sensitivity 0–7;
/// the output level 0–99; the mode 0–1; the coarse frequency 0–31; the fine
/// frequency 0–99; the detune 0–14.
const OPERATOR_LARGEST: [u8; 21] = [
    99, 99, 99, 99, 99, 99, 99, 99, 99, 99, 99, 3, 3, 7, 3, 7, 99, 1, 31, 99, 14,
];

/// The largest step of each of the voice's own 19 parameters, 126 to 144: the
/// pitch envelope's rates and levels 0–99; the algorithm 0–31; the feedback
/// 0–7; the oscillators' key sync 0–1; the LFO's speed, delay and pitch and
/// amplitude depths 0–99; its key sync 0–1; its wave 0–5; the pitch
/// sensitivity 0–7; the transposition 0–48.
const VOICE_LARGEST: [u8; 19] = [99, 99, 99, 99, 99, 99, 99, 99, 31, 7, 1, 99, 99, 99, 99, 1, 5, 7, 48];

/// The largest step of the parameter of a single-voice dump's byte of this
/// number, below 155; the ten bytes of the name take any seven bits.
pub fn largest(number: usize) -> u8 {
    match number {
        0..126 => OPERATOR_LARGEST[number % 21],
        126..145 => VOICE_LARGEST[number - 126],
        _ => 0x7f,
    }
}

/// The largest step of the voice parameter of this number, below 156, in a
/// parameter change: a single-voice dump's, and for 155, which only a parameter
/// change carries, the operators' switches, a bit each in the low six bits
/// \[DX7-MANUAL\] p. 31.
pub fn largest_in_change(number: usize) -> u8 {
    if number == 155 { 0x3f } else { largest(number) }
}

/// The largest step of each function parameter, 64 to 77 \[DX7-MANUAL\] p. 31:
/// mono/poly 0–1; the pitch bend's range and step 0–12; the portamento's mode
/// and glissando 0–1 and its time 0–99; then the range, 0–99, and the
/// assignment, 0–7, of the modulation wheel, the foot controller, the breath
/// controller and the aftertouch.
pub const FUNCTION_LARGEST: [u8; 14] = [1, 12, 12, 1, 1, 99, 99, 7, 99, 7, 99, 7, 99, 7];

/// Where each of an operator's 21 parameters lies in its 17 bytes of a packed
/// voice \[DEXED-SYSEX\], and at which bit: R1 to R4 and L1 to L4 in bytes 0
/// to 7; the break point and the depths in 8 to 10; the left and right curves
/// in 11 at bits 0 and 2; the rate scaling in 12 at bit 0; the amplitude and
/// velocity sensitivities in 13 at bits 0 and 2; the output level in 14; the
/// mode and the coarse frequency in 15 at bits 0 and 1; the fine frequency in
/// 16; the detune in 12 at bit 3.
const OPERATOR_PACKING: [(usize, u8); 21] = [
    (0, 0),
    (1, 0),
    (2, 0),
    (3, 0),
    (4, 0),
    (5, 0),
    (6, 0),
    (7, 0),
    (8, 0),
    (9, 0),
    (10, 0),
    (11, 0),
    (11, 2),
    (12, 0),
    (13, 0),
    (13, 2),
    (14, 0),
    (15, 0),
    (15, 1),
    (16, 0),
    (12, 3),
];

/// Where each of the voice's own 19 parameters lies in a packed voice, and at
/// which bit: the pitch envelope in bytes 102 to 109; the algorithm in 110; the
/// feedback and the oscillators' key sync in 111 at bits 0 and 3; the LFO's
/// speed, delay and depths in 112 to 115; its key sync and wave and the pitch
/// sensitivity in 116 at bits 0, 1 and 4; the transposition in 117.
const VOICE_PACKING: [(usize, u8); 19] = [
    (102, 0),
    (103, 0),
    (104, 0),
    (105, 0),
    (106, 0),
    (107, 0),
    (108, 0),
    (109, 0),
    (110, 0),
    (111, 0),
    (111, 3),
    (112, 0),
    (113, 0),
    (114, 0),
    (115, 0),
    (116, 0),
    (116, 1),
    (116, 4),
    (117, 0),
];

/// The 128 bytes of a packed voice of a single-voice dump's 155, laid out by
/// the tables above; the name's ten bytes are its last ten.
pub fn pack(steps: &[u8]) -> Vec<u8> {
    assert_eq!(steps.len(), 155);
    let mut packed = vec![0; 128];
    for (number, &step) in steps.iter().enumerate() {
        let (byte, bit) = match number {
            0..126 => {
                let (byte, bit) = OPERATOR_PACKING[number % 21];
                (17 * (number / 21) + byte, bit)
            }
            126..145 => VOICE_PACKING[number - 126],
            _ => (number - 145 + 118, 0),
        };
        packed[byte] |= step << bit;
    }
    packed
}

/// The checksum of a dump's data: what makes their sum with it a multiple of
/// 128.
pub fn checksum(data: &[u8]) -> u8 {
    let sum: u32 = data.iter().map(|&byte| u32::from(byte)).sum();
    u8::try_from((128 - sum % 128) % 128).expect("below 128")
}

/// A dump's message: F0, Yamaha's ID, the sub-status 0 and the channel, the
/// format, the byte count in two bytes of seven bits, the data, their checksum
/// and F7.
pub fn dump(channel: u8, format: u8, data: &[u8]) -> Vec<u8> {
    let count = data.len();
    let mut message = vec![
        0xf0,
        0x43,
        channel,
        format,
        u8::try_from(count >> 7).expect("a count below 2^14"),
        u8::try_from(count & 0x7f).expect("seven bits"),
    ];
    message.extend_from_slice(data);
    message.push(checksum(data));
    message.push(0xf7);
    message
}

/// The 155 bytes of a single-voice dump of random steps, each within its
/// range.
pub fn random_steps(random: &mut SplitMix64) -> Vec<u8> {
    (0..155).map(|number| random.up_to(largest(number))).collect()
}

/// A valid single-voice dump of random steps, on a channel.
pub fn random_voice(random: &mut SplitMix64, channel: u8) -> Vec<u8> {
    dump(channel, 0, &random_steps(random))
}

/// The steps of 32 random voices, and a valid 32-voice dump of them, on a
/// channel.
pub fn random_bank(random: &mut SplitMix64, channel: u8) -> (Vec<Vec<u8>>, Vec<u8>) {
    let steps: Vec<Vec<u8>> = (0..32).map(|_| random_steps(random)).collect();
    let data: Vec<u8> = steps.iter().flat_map(|steps| pack(steps)).collect();
    let message = dump(channel, 9, &data);
    (steps, message)
}

/// The largest step of each byte of a side's thirty in the dump of one
/// performance, as \[TX7-MANUAL\] p. 42 gives them: mono/poly at 2 0–1; the
/// pitch bend's range and step at 3 and 4 0–12; the portamento's time at 5
/// 0–99, glissando and mode at 6 and 7 0–1; the sensitivity 0–15 and the
/// assignment 0–7 of the modulation wheel, the foot controller, the aftertouch
/// and the breath controller at 9 to 16; the attenuation at 26 0–7. The bytes
/// it leaves unnamed take any seven bits.
const SIDE_LARGEST: [u8; 30] = [
    0x7f, 0x7f, 1, 12, 12, 99, 1, 1, 0x7f, 15, 7, 15, 7, 15, 7, 15, 7, 0x7f, 0x7f, 0x7f, 0x7f, 0x7f, 0x7f, 0x7f, 0x7f,
    0x7f, 7, 0x7f, 0x7f, 0x7f,
];

/// The largest step of the byte of this number, below 94, of the dump of one
/// performance: side A's thirty, side B's, the voice memory select at 61 0–1,
/// and the name at 64 to 93, which, with the bytes left unnamed, takes any
/// seven bits.
pub fn performance_largest(number: usize) -> u8 {
    match number {
        0..=59 => SIDE_LARGEST[number % 30],
        61 => 1,
        _ => 0x7f,
    }
}

/// Where some of a field's bits lie: their byte, the bit there of the lowest of
/// them, that bit's place in the field, and how many they are.
type Bits = (usize, u8, u8, u8);

/// Where each of a side's fields lies in its sixteen bytes of a packed
/// performance \[TX7-MANUAL\] p. 43, by its byte in the dump of one: a list of
/// (byte, bit, the field's own lowest bit, width). Mono/poly in byte 0 at bit
/// 6; the pitch bend's range in byte 1 at bit 0, and its step's low three bits
/// in byte 1 at bit 4 and its high bit in byte 15 at bit 6; the portamento's
/// time in byte 2, its glissando and mode in byte 3 at bits 0 and 1; the
/// sensitivity and the assignment of the four controllers in bytes 4 to 7 at
/// bits 0 and 4; the attenuation in byte 14 at bit 0.
const SIDE_PACKING: [(usize, &[Bits]); 15] = [
    (2, &[(0, 6, 0, 1)]),
    (3, &[(1, 0, 0, 4)]),
    (4, &[(1, 4, 0, 3), (15, 6, 3, 1)]),
    (5, &[(2, 0, 0, 7)]),
    (6, &[(3, 0, 0, 1)]),
    (7, &[(3, 1, 0, 1)]),
    (9, &[(4, 0, 0, 4)]),
    (10, &[(4, 4, 0, 3)]),
    (11, &[(5, 0, 0, 4)]),
    (12, &[(5, 4, 0, 3)]),
    (13, &[(6, 0, 0, 4)]),
    (14, &[(6, 4, 0, 3)]),
    (15, &[(7, 0, 0, 4)]),
    (16, &[(7, 4, 0, 3)]),
    (26, &[(14, 0, 0, 3)]),
];

/// The 64 bytes of a packed performance of the 94 steps of the dump of one and
/// a key assign mode, laid out by the table above: side A in bytes 0 to 15,
/// side B in 16 to 31, the voice memory select in byte 32 at bit 2 and the key
/// assign mode at bit 0, the name in 34 to 63. The unnamed bytes are not held.
pub fn pack_performance(steps: &[u8], key_assign: u8) -> Vec<u8> {
    assert_eq!(steps.len(), 94);
    let mut packed = vec![0; 64];
    for side in 0..2 {
        for (offset, places) in SIDE_PACKING {
            let step = steps[30 * side + offset];
            for &(byte, bit, low, width) in places {
                packed[16 * side + byte] |= (step >> low & ((1 << width) - 1)) << bit;
            }
        }
    }
    packed[32] = steps[61] << 2 | key_assign;
    packed[34..].copy_from_slice(&steps[64..]);
    packed
}

/// The bits that each of a packed performance's 64 bytes uses, by the table
/// above: the sides' fields, the voice memory select and the key assign mode in
/// byte 32's bits 2–0, and the name in 34 to 63; the others are unused.
pub fn performance_used() -> Vec<u8> {
    let mut used = vec![0; 64];
    for side in 0..2 {
        for (_, places) in SIDE_PACKING {
            for &(byte, bit, _, width) in places {
                used[16 * side + byte] |= ((1 << width) - 1) << bit;
            }
        }
    }
    used[32] = 0x07;
    for byte in &mut used[34..] {
        *byte = 0x7f;
    }
    used
}

/// The 94 bytes of the dump of one performance of random steps, each within
/// its range.
pub fn random_performance_steps(random: &mut SplitMix64) -> Vec<u8> {
    (0..94)
        .map(|number| random.up_to(performance_largest(number)))
        .collect()
}

/// The steps of random performances, as the dump of 64 holds them, with no
/// unnamed byte but 0, and random key assign modes; and a valid dump of 64 of
/// them, on a channel.
pub fn random_performances(random: &mut SplitMix64, channel: u8) -> (Vec<(Vec<u8>, u8)>, Vec<u8>) {
    let performances: Vec<(Vec<u8>, u8)> = (0..64)
        .map(|_| {
            let steps = (0..94)
                .map(|number| {
                    let named = SIDE_PACKING
                        .iter()
                        .any(|&(offset, _)| number < 60 && number % 30 == offset);
                    if named || number == 61 || number >= 64 {
                        random.up_to(performance_largest(number))
                    } else {
                        0
                    }
                })
                .collect();
            (steps, random.up_to(2))
        })
        .collect();
    let data: Vec<u8> = performances
        .iter()
        .flat_map(|(steps, key_assign)| pack_performance(steps, *key_assign))
        .collect();
    let message = dump(channel, 2, &data);
    (performances, message)
}

/// The largest step of each of the DX7II supplement's 49 parameters, in the
/// order of the ACED format \[DX7II-MANUAL\] p. Add-9, parameters 0 to 38 and
/// 64 to 73: the operators' scaling modes 0–1 and amplitude modulation
/// sensitivities 0–7, OP6's first; the pitch envelope's range 0–3, the LFO's
/// key trigger and velocity's switch of the pitch envelope 0–1, mono/poly with
/// unison 0–3; the pitch bend's range and step 0–12 and mode 0–3, as
/// \[DX7S-MANUAL\] p. Add-10 has it, normal at 0 among them; the random
/// pitch 0–7; the portamento's mode 0–1, step 0–12 and time 0–99; the
/// modulation wheel's three ranges, the first foot controller's four, the
/// breath controller's and the aftertouch's three with a pitch bias 0–100;
/// the pitch envelope's rate scaling 0–7; the second foot controller's and
/// the MIDI controller's four ranges; the unison's detune 0–7; and the first
/// foot controller's switch to the slider 0–1.
const SUPPLEMENT_LARGEST: [u8; 49] = [
    1, 1, 1, 1, 1, 1, 7, 7, 7, 7, 7, 7, 3, 1, 1, 3, 12, 12, 3, 7, 1, 12, 99, 99, 99, 99, 99, 99, 99, 99, 99, 99, 99,
    100, 99, 99, 99, 100, 7, 99, 99, 99, 99, 99, 99, 99, 99, 7, 1,
];

/// The largest step of the supplement's parameter at this place of the ACED
/// format.
pub fn supplement_largest(index: usize) -> u8 {
    SUPPLEMENT_LARGEST[index]
}

/// Where the supplement's parameter at this place of the ACED format lies in a
/// packed supplement \[DX7II-MANUAL\] p. Add-12, its byte and its lowest bit:
/// the scaling modes of OP6 to OP1 in byte 0 at bits 0 to 5; the amplitude
/// modulation sensitivities of OP6 and OP5 in byte 1 at bits 0 and 3, of OP4
/// and OP3 in byte 2, of OP2 and OP1 in byte 3; the pitch envelope's range,
/// the LFO's key trigger, velocity's switch and the random pitch in byte 4 at
/// bits 0, 2, 3 and 4; mono/poly with unison and the pitch bend's range in
/// byte 5 at bits 0 and 2; its step and mode in byte 6 at bits 0 and 4; the
/// portamento's mode and step in byte 7 at bits 0 and 1; the portamento's
/// time and the ranges to the aftertouch's pitch bias in bytes 8 to 23; the
/// pitch envelope's rate scaling in byte 24; the second foot controller's and
/// the MIDI controller's ranges in bytes 26 to 33; the unison's detune and the
/// slider's switch in byte 34 at bits 0 and 3.
pub fn supplement_place(index: usize) -> (usize, u32) {
    match index {
        0..=5 => (0, u32::try_from(index).expect("below 6")),
        6 => (1, 0),
        7 => (1, 3),
        8 => (2, 0),
        9 => (2, 3),
        10 => (3, 0),
        11 => (3, 3),
        12 => (4, 0),
        13 => (4, 2),
        14 => (4, 3),
        15 => (5, 0),
        16 => (5, 2),
        17 => (6, 0),
        18 => (6, 4),
        19 => (4, 4),
        20 => (7, 0),
        21 => (7, 1),
        22..=37 => (index - 14, 0),
        38 => (24, 0),
        39..=46 => (index - 13, 0),
        47 => (34, 0),
        _ => (34, 3),
    }
}

/// The bits of a field whose largest step is this.
fn width(largest: u8) -> u32 {
    largest.bit_width()
}

/// The 35 bytes of a packed supplement of the 49 steps of the ACED format,
/// laid out by [`supplement_place`].
pub fn pack_supplement(steps: &[u8]) -> Vec<u8> {
    assert_eq!(steps.len(), 49);
    let mut packed = vec![0; 35];
    for (index, &step) in steps.iter().enumerate() {
        let (byte, bit) = supplement_place(index);
        packed[byte] |= step << bit;
    }
    packed
}

/// The bits that each of a packed supplement's 35 bytes uses, by
/// [`supplement_place`] and each field's width; the others are unused.
pub fn supplement_used() -> Vec<u8> {
    let mut used = vec![0; 35];
    for (index, &largest) in SUPPLEMENT_LARGEST.iter().enumerate() {
        let (byte, bit) = supplement_place(index);
        used[byte] |= ((1 << width(largest)) - 1) << bit;
    }
    used
}

/// The 49 bytes of the dump of one supplement of random steps, each within its
/// range.
pub fn random_supplement_steps(random: &mut SplitMix64) -> Vec<u8> {
    SUPPLEMENT_LARGEST
        .iter()
        .map(|&largest| random.up_to(largest))
        .collect()
}

/// The steps of 32 random supplements, and a valid dump of 32 of them, on a
/// channel.
pub fn random_supplements(random: &mut SplitMix64, channel: u8) -> (Vec<Vec<u8>>, Vec<u8>) {
    let steps: Vec<Vec<u8>> = (0..32).map(|_| random_supplement_steps(random)).collect();
    let data: Vec<u8> = steps.iter().flat_map(|steps| pack_supplement(steps)).collect();
    let message = dump(channel, 6, &data);
    (steps, message)
}

/// The smallest and largest step of each of the DX7II performance's 51
/// parameters, in the order of the PCED format, as \[DX7II-MANUAL\] p. Add-10
/// and \[DX7S-MANUAL\] p. Add-11 give them, the wider where they differ: the
/// play mode 0–2; the voices' numbers 0–127; the micro tuning's table 0–74,
/// key 0–11 and switches 0–3; the dual detune 0–7; the split point 0–127; the
/// forced damp 0–1; the sustain switches, the foot switch's assignment and
/// switches 0–3; the soft pedal's range 0–7; the note shifts 0–48; the balance
/// 0–100 and the total volume 0–99; the sliders' assignments 0–105 and 0–109
/// and their switches 0–15; the pan's mode 0–3, range 0–99, assignment 0–2
/// and envelope 0–99; the name's twenty bytes of seven bits.
const DX7II_PERFORMANCE_LARGEST: [u8; 31] = [
    2, 127, 127, 74, 11, 3, 7, 127, 1, 3, 3, 3, 7, 48, 48, 100, 99, 105, 109, 15, 3, 99, 2, 99, 99, 99, 99, 99, 99, 99,
    99,
];

/// The largest step of the DX7II performance's parameter of this number, below
/// 51.
pub fn dx7ii_performance_largest(number: usize) -> u8 {
    DX7II_PERFORMANCE_LARGEST.get(number).copied().unwrap_or(0x7f)
}

/// The range of each of the DX7II system set-up's first 20 bytes, its
/// parameters 64 to 83 \[DX7II-MANUAL\] p. Add-13 \[DX7S-MANUAL\] p. Add-14,
/// the wider where they differ: the transmit channel 0–15; its switch 0–1; the
/// receive channels 0–16; the omni 0–1; the MIDI controllers' numbers 9–31 and
/// the sliders' 5–31 (11–31 on the DX7II); the key filter and the program
/// change mode 0–2; the local switch and the blocks 0–1; the system channel
/// 0–15 and its switch 0–1; the cartridge banks 0–15; the memory protect 0–3.
/// The bytes after them, 20 to 101, take any seven bits.
const SYSTEM_RANGES: [(u8, u8); 20] = [
    (0, 15),
    (0, 1),
    (0, 16),
    (0, 16),
    (0, 1),
    (9, 31),
    (9, 31),
    (5, 31),
    (5, 31),
    (0, 2),
    (0, 2),
    (0, 1),
    (0, 1),
    (0, 1),
    (0, 15),
    (0, 1),
    (0, 15),
    (0, 15),
    (0, 15),
    (0, 3),
];

/// The smallest and largest step of the DX7II system set-up's byte of this
/// number, below 102.
pub fn system_range(byte: usize) -> (u8, u8) {
    SYSTEM_RANGES.get(byte).copied().unwrap_or((0, 0x7f))
}

/// A universal bulk dump of one group, on a channel \[DX7II-MANUAL\]: F0, 43,
/// the sub-status 0 and the channel, 7E, the byte count of the names and the
/// data, the classification name and the data format name, the data, the
/// checksum of the names and the data, F7.
pub fn universal(channel: u8, names: &[u8; 10], data: &[u8]) -> Vec<u8> {
    let counted: Vec<u8> = names.iter().chain(data).copied().collect();
    let count = counted.len();
    let mut message = vec![
        0xf0,
        0x43,
        channel,
        0x7e,
        u8::try_from(count >> 7).expect("a count below 2^14"),
        u8::try_from(count & 0x7f).expect("seven bits"),
    ];
    message.extend_from_slice(&counted);
    message.push(checksum(&counted));
    message.push(0xf7);
    message
}

/// The 51 bytes of a DX7II performance of random steps, each within its range.
pub fn random_dx7ii_performance(random: &mut SplitMix64) -> Vec<u8> {
    (0..51)
        .map(|number| random.up_to(dx7ii_performance_largest(number)))
        .collect()
}

/// The 102 bytes of a DX7II system set-up of random steps, each within its
/// range.
pub fn random_system_setup(random: &mut SplitMix64) -> Vec<u8> {
    (0..102)
        .map(|byte| {
            let (smallest, largest) = system_range(byte);
            smallest + random.up_to(largest - smallest)
        })
        .collect()
}

/// The 85 bytes of a DX7s system set-up of random steps, each within its
/// range: the DX7II's but for the 17, 20 to 36, that have no name
/// \[DX7S-MANUAL\].
pub fn random_dx7s_system_setup(random: &mut SplitMix64) -> Vec<u8> {
    let steps = random_system_setup(random);
    [&steps[..20], &steps[37..]].concat()
}

/// A universal bulk dump of a group for each of `groups`, on a channel, each
/// group as [`universal`] writes it: its count, the names, its data and the
/// checksum of the names and the data \[DX7II-MANUAL\] p. Add-4.
pub fn universal_groups(channel: u8, names: &[u8; 10], groups: &[Vec<u8>]) -> Vec<u8> {
    let mut message = vec![0xf0, 0x43, channel, 0x7e];
    for data in groups {
        let one = universal(channel, names, data);
        // A group is what lies between the header's four bytes and F7.
        message.extend_from_slice(&one[4..one.len() - 1]);
    }
    message.push(0xf7);
    message
}

/// The largest step of a key of a micro tuning: 84 · 128 + 42
/// \[DX7II-MANUAL\] p. Add-14.
pub const MICRO_TUNING_LARGEST: u16 = 10794;

/// The 256 bytes of a micro tuning of the 128 keys' steps: each step's high
/// seven bits, then its low seven.
pub fn micro_tuning_bytes(steps: &[u16; 128]) -> Vec<u8> {
    steps
        .iter()
        .flat_map(|&step| {
            [
                u8::try_from(step >> 7).expect("a step below 2^14"),
                u8::try_from(step & 0x7f).expect("seven bits"),
            ]
        })
        .collect()
}

/// The steps of a micro tuning's 128 keys at random, each no more than 10794.
pub fn random_micro_tuning_steps(random: &mut SplitMix64) -> [u16; 128] {
    std::array::from_fn(|_| u16::try_from(random.below(usize::from(MICRO_TUNING_LARGEST) + 1)).expect("below 10795"))
}

/// The 492 bytes of a fractional scaling of its 246 bytes, in the order of the
/// dump: each byte's high four bits and then its low four, each sent as the
/// byte 0x30 above them \[DX7S-MANUAL\] p. Add-4.
pub fn scaling_bytes(bytes: &[u8; 246]) -> Vec<u8> {
    bytes
        .iter()
        .flat_map(|&byte| [0x30 + (byte >> 4), 0x30 + (byte & 0x0f)])
        .collect()
}

/// The 246 bytes of a fractional scaling at random: for each operator from
/// OP6, its offset's byte and the levels of its 40 groups of keys, any byte
/// each \[DX7II-MANUAL\] p. Add-15.
pub fn random_scaling_bytes(random: &mut SplitMix64) -> [u8; 246] {
    std::array::from_fn(|_| random.byte())
}
