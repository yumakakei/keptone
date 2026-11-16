// SPDX-FileCopyrightText: 2026 Yuma Kakei <yumasansansan@gmail.com>
// SPDX-License-Identifier: GPL-3.0-or-later
//
// This file is part of Keptone. The SPDX lines name its copyright holder and its
// license, in the machine-readable form of the REUSE specification: the GNU
// General Public License, version 3 or any later version
// (LICENSES/GPL-3.0-or-later.txt).
//
// Written by scripts/minimax.py from scripts/minimax.sollya; do not edit.
// The bounds are those of Sollya 8.0.

//! The coefficients of the polynomials, as Sollya's fpminimax designed them in
//! binary64, the leading one of each a double-double (high and low parts), and
//! the error of each polynomial, with its coefficients as they are, as Sollya's
//! supnorm certified it: an upper bound over the whole interval.

// sin 2 pi t on |t| <= 1/8, odd: t (c1 + c3 t^2 + ... + c15 t^14).
// Certified relative error ≤ 7.05e-20 (7.04532152060832100422602250130998946531235560541972517967224e-20).
/// The coefficient of power 1, high part.
pub(crate) const SIN_2PI_1_HI: f64 = f64::from_bits(0x4019_21fb_5444_2d18); // 0x1.921fb54442d18p2
/// The coefficient of power 1, low part.
pub(crate) const SIN_2PI_1_LO: f64 = f64::from_bits(0x3cb1_ae50_a6ee_fce5); // 0x1.1ae50a6eefce5p-52
/// The coefficient of power 3.
pub(crate) const SIN_2PI_3: f64 = f64::from_bits(0xc044_abbc_e625_be53); // -0x1.4abbce625be53p5
/// The coefficient of power 5.
pub(crate) const SIN_2PI_5: f64 = f64::from_bits(0x4054_66bc_6775_ab46); // 0x1.466bc6775ab46p6
/// The coefficient of power 7.
pub(crate) const SIN_2PI_7: f64 = f64::from_bits(0xc053_2d2c_ce63_37ea); // -0x1.32d2cce6337eap6
/// The coefficient of power 9.
pub(crate) const SIN_2PI_9: f64 = f64::from_bits(0x4045_0783_490f_1be5); // 0x1.50783490f1be5p5
/// The coefficient of power 11.
pub(crate) const SIN_2PI_11: f64 = f64::from_bits(0xc02e_3075_a501_9d1d); // -0x1.e3075a5019d1dp3
/// The coefficient of power 13.
pub(crate) const SIN_2PI_13: f64 = f64::from_bits(0x400e_8f90_2029_18b1); // 0x1.e8f90202918b1p1
/// The coefficient of power 15.
pub(crate) const SIN_2PI_15: f64 = f64::from_bits(0xbfe6_f1cc_0b35_b05c); // -0x1.6f1cc0b35b05cp-1

// cos 2 pi t - 1 on |t| <= 1/8, even: t^2 (c2 + c4 t^2 + ... + c16 t^14).
// Certified relative error ≤ 5.09e-20 (5.089953471120767004441160950498068693548248120350763201713562e-20).
/// The coefficient of power 2, high part.
pub(crate) const COS_2PI_2_HI: f64 = f64::from_bits(0xc033_bd3c_c9be_45de); // -0x1.3bd3cc9be45dep4
/// The coefficient of power 2, low part.
pub(crate) const COS_2PI_2_LO: f64 = f64::from_bits(0xbcd6_9759_3c4f_07e9); // -0x1.697593c4f07e9p-50
/// The coefficient of power 4.
pub(crate) const COS_2PI_4: f64 = f64::from_bits(0x4050_3c1f_081b_5ac4); // 0x1.03c1f081b5ac4p6
/// The coefficient of power 6.
pub(crate) const COS_2PI_6: f64 = f64::from_bits(0xc055_5d3c_7e3c_c0ee); // -0x1.55d3c7e3cc0eep6
/// The coefficient of power 8.
pub(crate) const COS_2PI_8: f64 = f64::from_bits(0x404e_1f50_6894_2cba); // 0x1.e1f5068942cbap5
/// The coefficient of power 10.
pub(crate) const COS_2PI_10: f64 = f64::from_bits(0xc03a_6d1f_2d3a_7f65); // -0x1.a6d1f2d3a7f65p4
/// The coefficient of power 12.
pub(crate) const COS_2PI_12: f64 = f64::from_bits(0x401f_9d3c_c168_270e); // 0x1.f9d3cc168270ep2
/// The coefficient of power 14.
pub(crate) const COS_2PI_14: f64 = f64::from_bits(0xbffb_70d9_a2a6_3945); // -0x1.b70d9a2a63945p0
/// The coefficient of power 16.
pub(crate) const COS_2PI_16: f64 = f64::from_bits(0x3fd2_b354_4830_4372); // 0x1.2b35448304372p-2

// 2^r - 1 on |r| <= 1/2: r (c1 + c2 r + ... + c12 r^11).
// Certified absolute error ≤ 9.31e-20 (9.300283078852644379424065897848938888614611641969531774520874e-20).
/// The coefficient of power 1, high part.
pub(crate) const EXP2_1_HI: f64 = f64::from_bits(0x3fe6_2e42_fefa_39ef); // 0x1.62e42fefa39efp-1
/// The coefficient of power 1, low part.
pub(crate) const EXP2_1_LO: f64 = f64::from_bits(0x3c79_7a19_82f8_0946); // 0x1.97a1982f80946p-56
/// The coefficient of power 2.
pub(crate) const EXP2_2: f64 = f64::from_bits(0x3fce_bfbd_ff82_c58f); // 0x1.ebfbdff82c58fp-3
/// The coefficient of power 3.
pub(crate) const EXP2_3: f64 = f64::from_bits(0x3fac_6b08_d704_a0d1); // 0x1.c6b08d704a0d1p-5
/// The coefficient of power 4.
pub(crate) const EXP2_4: f64 = f64::from_bits(0x3f83_b2ab_6fba_4d80); // 0x1.3b2ab6fba4d8p-7
/// The coefficient of power 5.
pub(crate) const EXP2_5: f64 = f64::from_bits(0x3f55_d87f_e78a_21cf); // 0x1.5d87fe78a21cfp-10
/// The coefficient of power 6.
pub(crate) const EXP2_6: f64 = f64::from_bits(0x3f24_3091_2f8a_936c); // 0x1.430912f8a936cp-13
/// The coefficient of power 7.
pub(crate) const EXP2_7: f64 = f64::from_bits(0x3eef_fcbf_c762_6dc3); // 0x1.ffcbfc7626dc3p-17
/// The coefficient of power 8.
pub(crate) const EXP2_8: f64 = f64::from_bits(0x3eb6_2c02_1638_6490); // 0x1.62c021638649p-20
/// The coefficient of power 9.
pub(crate) const EXP2_9: f64 = f64::from_bits(0x3e7b_524d_ad6b_8ae8); // 0x1.b524dad6b8ae8p-24
/// The coefficient of power 10.
pub(crate) const EXP2_10: f64 = f64::from_bits(0x3e3e_4d20_0ac7_ba29); // 0x1.e4d200ac7ba29p-28
/// The coefficient of power 11.
pub(crate) const EXP2_11: f64 = f64::from_bits(0x3dfe_a04c_c290_1f05); // 0x1.ea04cc2901f05p-32
/// The coefficient of power 12.
pub(crate) const EXP2_12: f64 = f64::from_bits(0x3dbc_0e44_703c_7519); // 0x1.c0e44703c7519p-36

// the derivative of sinc u on |u| < 1/2, odd: u (c1 + c3 u^2 + ... + c19 u^18).
// Certified relative error ≤ 1.02e-19 (1.0178679742254629165480305647006886715644213836640119552612305e-19).
/// The coefficient of power 1, high part.
pub(crate) const SINC_PRIME_1_HI: f64 = f64::from_bits(0xc00a_51a6_6253_07d3); // -0x1.a51a6625307d3p1
/// The coefficient of power 1, low part.
pub(crate) const SINC_PRIME_1_LO: f64 = f64::from_bits(0xbc91_6e88_1cbb_552e); // -0x1.16e881cbb552ep-54
/// The coefficient of power 3.
pub(crate) const SINC_PRIME_3: f64 = f64::from_bits(0x4009_f9cb_402b_c46c); // 0x1.9f9cb402bc46cp1
/// The coefficient of power 5.
pub(crate) const SINC_PRIME_5: f64 = f64::from_bits(0xbff2_4fea_b558_a458); // -0x1.24feab558a458p0
/// The coefficient of power 7.
pub(crate) const SINC_PRIME_7: f64 = f64::from_bits(0x3fca_c680_5cf3_176b); // 0x1.ac6805cf3176bp-3
/// The coefficient of power 9.
pub(crate) const SINC_PRIME_9: f64 = f64::from_bits(0xbf98_061c_54b7_3c3d); // -0x1.8061c54b73c3dp-6
/// The coefficient of power 11.
pub(crate) const SINC_PRIME_11: f64 = f64::from_bits(0x3f5d_2eaa_5a6a_8eee); // 0x1.d2eaa5a6a8eeep-10
/// The coefficient of power 13.
pub(crate) const SINC_PRIME_13: f64 = f64::from_bits(0xbf19_99f8_1ad1_a856); // -0x1.999f81ad1a856p-14
/// The coefficient of power 15.
pub(crate) const SINC_PRIME_15: f64 = f64::from_bits(0x3ed0_faaf_d17c_b516); // 0x1.0faafd17cb516p-18
/// The coefficient of power 17.
pub(crate) const SINC_PRIME_17: f64 = f64::from_bits(0xbe81_29dd_493a_c562); // -0x1.129dd493ac562p-23
/// The coefficient of power 19.
pub(crate) const SINC_PRIME_19: f64 = f64::from_bits(0x3df6_accd_756d_a7a6); // 0x1.6accd756da7a6p-32

// sinc u - 1 on |u| < 2^-6, the Taylor polynomial: u^2 (c2 + c4 u^2 + ... + c12 u^10).
// Certified relative error ≤ 7.43e-21 (7.42437574384103882213010354787496858364193030865862965583801e-21).
/// The coefficient of power 2.
pub(crate) const SINC_TAYLOR_2: f64 = f64::from_bits(0xbffa_51a6_6253_07d3); // -0x1.a51a6625307d3p0
/// The coefficient of power 4.
pub(crate) const SINC_TAYLOR_4: f64 = f64::from_bits(0x3fe9_f9cb_402b_c46c); // 0x1.9f9cb402bc46cp-1
/// The coefficient of power 6.
pub(crate) const SINC_TAYLOR_6: f64 = f64::from_bits(0xbfc8_6a8e_4720_db67); // -0x1.86a8e4720db67p-3
/// The coefficient of power 8.
pub(crate) const SINC_TAYLOR_8: f64 = f64::from_bits(0x3f9a_c680_5cf3_50a6); // 0x1.ac6805cf350a6p-6
/// The coefficient of power 10.
pub(crate) const SINC_TAYLOR_10: f64 = f64::from_bits(0xbf63_3816_aa46_07ab); // -0x1.33816aa4607abp-9
/// The coefficient of power 12.
pub(crate) const SINC_TAYLOR_12: f64 = f64::from_bits(0x3f23_7471_9fab_3915); // 0x1.374719fab3915p-13

// π.
/// The coefficient of power 0, high part.
pub(crate) const PI_0_HI: f64 = f64::from_bits(0x4009_21fb_5444_2d18); // 0x1.921fb54442d18p1
/// The coefficient of power 0, low part.
pub(crate) const PI_0_LO: f64 = f64::from_bits(0x3ca1_a626_3314_5c07); // 0x1.1a62633145c07p-53
