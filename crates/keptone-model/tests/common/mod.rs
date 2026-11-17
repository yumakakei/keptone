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
