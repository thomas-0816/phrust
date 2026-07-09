const D0: [u32; 8] = [
    0x243f_6a88,
    0x85a3_08d3,
    0x1319_8a2e,
    0x0370_7344,
    0xa409_3822,
    0x299f_31d0,
    0x082e_fa98,
    0xec4e_6c89,
];

const K2: [u32; 32] = [
    0x4528_21e6,
    0x38d0_1377,
    0xbe54_66cf,
    0x34e9_0c6c,
    0xc0ac_29b7,
    0xc97c_50dd,
    0x3f84_d5b5,
    0xb547_0917,
    0x9216_d5d9,
    0x8979_fb1b,
    0xd131_0ba6,
    0x98df_b5ac,
    0x2ffd_72db,
    0xd01a_dfb7,
    0xb8e1_afed,
    0x6a26_7e96,
    0xba7c_9045,
    0xf12c_7f99,
    0x24a1_9947,
    0xb391_6cf7,
    0x0801_f2e2,
    0x858e_fc16,
    0x6369_20d8,
    0x7157_4e69,
    0xa458_fea3,
    0xf493_3d7e,
    0x0d95_748f,
    0x728e_b658,
    0x718b_cd58,
    0x8215_4aee,
    0x7b54_a41d,
    0xc25a_59b5,
];

const K3: [u32; 32] = [
    0x9c30_d539,
    0x2af2_6013,
    0xc5d1_b023,
    0x2860_85f0,
    0xca41_7918,
    0xb8db_38ef,
    0x8e79_dcb0,
    0x603a_180e,
    0x6c9e_0e8b,
    0xb01e_8a3e,
    0xd715_77c1,
    0xbd31_4b27,
    0x78af_2fda,
    0x5560_5c60,
    0xe655_25f3,
    0xaa55_ab94,
    0x5748_9862,
    0x63e8_1440,
    0x55ca_396a,
    0x2aab_10b6,
    0xb4cc_5c34,
    0x1141_e8ce,
    0xa154_86af,
    0x7c72_e993,
    0xb3ee_1411,
    0x636f_bc2a,
    0x2ba9_c55d,
    0x7418_31f6,
    0xce5c_3e16,
    0x9b87_931e,
    0xafd6_ba33,
    0x6c24_cf5c,
];

const K4: [u32; 32] = [
    0x7a32_5381,
    0x2895_8677,
    0x3b8f_4898,
    0x6b4b_b9af,
    0xc4bf_e81b,
    0x6628_2193,
    0x61d8_09cc,
    0xfb21_a991,
    0x487c_ac60,
    0x5dec_8032,
    0xef84_5d5d,
    0xe985_75b1,
    0xdc26_2302,
    0xeb65_1b88,
    0x2389_3e81,
    0xd396_acc5,
    0x0f6d_6ff3,
    0x83f4_4239,
    0x2e0b_4482,
    0xa484_2004,
    0x69c8_f04a,
    0x9e1f_9b5e,
    0x21c6_6842,
    0xf6e9_6c9a,
    0x670c_9c61,
    0xabd3_88f0,
    0x6a51_a0d2,
    0xd854_2f68,
    0x960f_a728,
    0xab51_33a3,
    0x6eef_0b6c,
    0x137a_3be4,
];

const K5: [u32; 32] = [
    0xba3b_f050,
    0x7efb_2a98,
    0xa1f1_651d,
    0x39af_0176,
    0x66ca_593e,
    0x8243_0e88,
    0x8cee_8619,
    0x456f_9fb4,
    0x7d84_a5c3,
    0x3b8b_5ebe,
    0xe06f_75d8,
    0x85c1_2073,
    0x401a_449f,
    0x56c1_6aa6,
    0x4ed3_aa62,
    0x363f_7706,
    0x1bfe_df72,
    0x429b_023d,
    0x37d0_d724,
    0xd00a_1248,
    0xdb0f_ead3,
    0x49f1_c09b,
    0x0753_72c9,
    0x8099_1b7b,
    0x25d4_79d8,
    0xf6e8_def7,
    0xe3fe_501a,
    0xb679_4c3b,
    0x976c_e0bd,
    0x04c0_06ba,
    0xc1a9_4fb6,
    0x409f_60c4,
];

const I2: [usize; 32] = [
    5, 14, 26, 18, 11, 28, 7, 16, 0, 23, 20, 22, 1, 10, 4, 8, 30, 3, 21, 9, 17, 24, 29, 6, 19, 12,
    15, 13, 2, 25, 31, 27,
];
const I3: [usize; 32] = [
    19, 9, 4, 20, 28, 17, 8, 22, 29, 14, 25, 12, 24, 30, 16, 26, 31, 15, 7, 3, 1, 0, 18, 27, 13, 6,
    21, 10, 23, 11, 5, 2,
];
const I4: [usize; 32] = [
    24, 4, 0, 14, 2, 7, 28, 23, 26, 6, 30, 20, 18, 25, 19, 3, 22, 11, 31, 21, 8, 27, 12, 9, 1, 29,
    5, 15, 17, 10, 16, 13,
];
const I5: [usize; 32] = [
    27, 3, 21, 26, 17, 11, 20, 29, 19, 0, 12, 7, 13, 8, 31, 10, 5, 9, 14, 30, 18, 6, 28, 24, 2, 23,
    16, 22, 4, 1, 25, 15,
];

const M0: [usize; 32] = [
    0, 7, 6, 5, 4, 3, 2, 1, 0, 7, 6, 5, 4, 3, 2, 1, 0, 7, 6, 5, 4, 3, 2, 1, 0, 7, 6, 5, 4, 3, 2, 1,
];
const M1: [usize; 32] = [
    1, 0, 7, 6, 5, 4, 3, 2, 1, 0, 7, 6, 5, 4, 3, 2, 1, 0, 7, 6, 5, 4, 3, 2, 1, 0, 7, 6, 5, 4, 3, 2,
];
const M2: [usize; 32] = [
    2, 1, 0, 7, 6, 5, 4, 3, 2, 1, 0, 7, 6, 5, 4, 3, 2, 1, 0, 7, 6, 5, 4, 3, 2, 1, 0, 7, 6, 5, 4, 3,
];
const M3: [usize; 32] = [
    3, 2, 1, 0, 7, 6, 5, 4, 3, 2, 1, 0, 7, 6, 5, 4, 3, 2, 1, 0, 7, 6, 5, 4, 3, 2, 1, 0, 7, 6, 5, 4,
];
const M4: [usize; 32] = [
    4, 3, 2, 1, 0, 7, 6, 5, 4, 3, 2, 1, 0, 7, 6, 5, 4, 3, 2, 1, 0, 7, 6, 5, 4, 3, 2, 1, 0, 7, 6, 5,
];
const M5: [usize; 32] = [
    5, 4, 3, 2, 1, 0, 7, 6, 5, 4, 3, 2, 1, 0, 7, 6, 5, 4, 3, 2, 1, 0, 7, 6, 5, 4, 3, 2, 1, 0, 7, 6,
];
const M6: [usize; 32] = [
    6, 5, 4, 3, 2, 1, 0, 7, 6, 5, 4, 3, 2, 1, 0, 7, 6, 5, 4, 3, 2, 1, 0, 7, 6, 5, 4, 3, 2, 1, 0, 7,
];
const M7: [usize; 32] = [
    7, 6, 5, 4, 3, 2, 1, 0, 7, 6, 5, 4, 3, 2, 1, 0, 7, 6, 5, 4, 3, 2, 1, 0, 7, 6, 5, 4, 3, 2, 1, 0,
];

pub(super) fn haval_digest(input: &[u8], output_bits: usize, passes: usize) -> Vec<u8> {
    debug_assert!(matches!(output_bits, 128 | 160 | 192 | 224 | 256));
    debug_assert!(matches!(passes, 3..=5));

    let mut state = D0;
    let mut chunks = input.chunks_exact(128);
    for chunk in &mut chunks {
        transform(&mut state, chunk, passes);
    }

    let bit_len = (input.len() as u64).wrapping_mul(8);
    let mut trailer = [0_u8; 10];
    trailer[0] = 0x01 | ((passes as u8 & 0x07) << 3) | ((output_bits as u8 & 0x03) << 6);
    trailer[1] = (output_bits >> 2) as u8;
    trailer[2..6].copy_from_slice(&(bit_len as u32).to_le_bytes());
    trailer[6..10].copy_from_slice(&((bit_len >> 32) as u32).to_le_bytes());

    let index = input.len() & 0x7f;
    let pad_len = if index < 118 {
        118 - index
    } else {
        246 - index
    };
    let mut final_blocks = Vec::with_capacity(chunks.remainder().len() + pad_len + trailer.len());
    final_blocks.extend_from_slice(chunks.remainder());
    final_blocks.push(1);
    final_blocks.resize(final_blocks.len() + pad_len.saturating_sub(1), 0);
    final_blocks.extend_from_slice(&trailer);
    debug_assert_eq!(final_blocks.len() % 128, 0);

    for chunk in final_blocks.chunks_exact(128) {
        transform(&mut state, chunk, passes);
    }

    fold_output(&mut state, output_bits);
    encode_state(&state, output_bits / 8)
}

fn transform(state: &mut [u32; 8], block: &[u8], passes: usize) {
    let mut x = [0_u32; 32];
    for (word, bytes) in x.iter_mut().zip(block.chunks_exact(4)) {
        *word = u32::from_le_bytes(bytes.try_into().expect("four-byte chunk"));
    }

    let mut e = *state;
    match passes {
        3 => transform3(&mut e, &x),
        4 => transform4(&mut e, &x),
        5 => transform5(&mut e, &x),
        _ => unreachable!("validated HAVAL pass count"),
    }
    for (state_word, e_word) in state.iter_mut().zip(e) {
        *state_word = state_word.wrapping_add(e_word);
    }
}

fn transform3(e: &mut [u32; 8], x: &[u32; 32]) {
    for i in 0..32 {
        round(
            e,
            i,
            f1(
                e[M1[i]], e[M0[i]], e[M3[i]], e[M5[i]], e[M6[i]], e[M2[i]], e[M4[i]],
            ),
            x[i],
            0,
        );
    }
    for i in 0..32 {
        round(
            e,
            i,
            f2(
                e[M4[i]], e[M2[i]], e[M1[i]], e[M0[i]], e[M5[i]], e[M3[i]], e[M6[i]],
            ),
            x[I2[i]],
            K2[i],
        );
    }
    for i in 0..32 {
        round(
            e,
            i,
            f3(
                e[M6[i]], e[M1[i]], e[M2[i]], e[M3[i]], e[M4[i]], e[M5[i]], e[M0[i]],
            ),
            x[I3[i]],
            K3[i],
        );
    }
}

fn transform4(e: &mut [u32; 8], x: &[u32; 32]) {
    for i in 0..32 {
        round(
            e,
            i,
            f1(
                e[M2[i]], e[M6[i]], e[M1[i]], e[M4[i]], e[M5[i]], e[M3[i]], e[M0[i]],
            ),
            x[i],
            0,
        );
    }
    for i in 0..32 {
        round(
            e,
            i,
            f2(
                e[M3[i]], e[M5[i]], e[M2[i]], e[M0[i]], e[M1[i]], e[M6[i]], e[M4[i]],
            ),
            x[I2[i]],
            K2[i],
        );
    }
    for i in 0..32 {
        round(
            e,
            i,
            f3(
                e[M1[i]], e[M4[i]], e[M3[i]], e[M6[i]], e[M0[i]], e[M2[i]], e[M5[i]],
            ),
            x[I3[i]],
            K3[i],
        );
    }
    for i in 0..32 {
        round(
            e,
            i,
            f4(
                e[M6[i]], e[M4[i]], e[M0[i]], e[M5[i]], e[M2[i]], e[M1[i]], e[M3[i]],
            ),
            x[I4[i]],
            K4[i],
        );
    }
}

fn transform5(e: &mut [u32; 8], x: &[u32; 32]) {
    for i in 0..32 {
        round(
            e,
            i,
            f1(
                e[M3[i]], e[M4[i]], e[M1[i]], e[M0[i]], e[M5[i]], e[M2[i]], e[M6[i]],
            ),
            x[i],
            0,
        );
    }
    for i in 0..32 {
        round(
            e,
            i,
            f2(
                e[M6[i]], e[M2[i]], e[M1[i]], e[M0[i]], e[M3[i]], e[M4[i]], e[M5[i]],
            ),
            x[I2[i]],
            K2[i],
        );
    }
    for i in 0..32 {
        round(
            e,
            i,
            f3(
                e[M2[i]], e[M6[i]], e[M0[i]], e[M4[i]], e[M3[i]], e[M1[i]], e[M5[i]],
            ),
            x[I3[i]],
            K3[i],
        );
    }
    for i in 0..32 {
        round(
            e,
            i,
            f4(
                e[M1[i]], e[M5[i]], e[M3[i]], e[M2[i]], e[M0[i]], e[M4[i]], e[M6[i]],
            ),
            x[I4[i]],
            K4[i],
        );
    }
    for i in 0..32 {
        round(
            e,
            i,
            f5(
                e[M2[i]], e[M5[i]], e[M0[i]], e[M6[i]], e[M4[i]], e[M3[i]], e[M1[i]],
            ),
            x[I5[i]],
            K5[i],
        );
    }
}

fn round(e: &mut [u32; 8], i: usize, f: u32, x: u32, k: u32) {
    let target = 7 - (i % 8);
    e[target] = f
        .rotate_right(7)
        .wrapping_add(e[M7[i]].rotate_right(11))
        .wrapping_add(x)
        .wrapping_add(k);
}

fn f1(x6: u32, x5: u32, x4: u32, x3: u32, x2: u32, x1: u32, x0: u32) -> u32 {
    (x1 & x4) ^ (x2 & x5) ^ (x3 & x6) ^ (x0 & x1) ^ x0
}

fn f2(x6: u32, x5: u32, x4: u32, x3: u32, x2: u32, x1: u32, x0: u32) -> u32 {
    (x1 & x2 & x3)
        ^ (x2 & x4 & x5)
        ^ (x1 & x2)
        ^ (x1 & x4)
        ^ (x2 & x6)
        ^ (x3 & x5)
        ^ (x4 & x5)
        ^ (x0 & x2)
        ^ x0
}

fn f3(x6: u32, x5: u32, x4: u32, x3: u32, x2: u32, x1: u32, x0: u32) -> u32 {
    (x1 & x2 & x3) ^ (x1 & x4) ^ (x2 & x5) ^ (x3 & x6) ^ (x0 & x3) ^ x0
}

fn f4(x6: u32, x5: u32, x4: u32, x3: u32, x2: u32, x1: u32, x0: u32) -> u32 {
    (x1 & x2 & x3)
        ^ (x2 & x4 & x5)
        ^ (x3 & x4 & x6)
        ^ (x1 & x4)
        ^ (x2 & x6)
        ^ (x3 & x4)
        ^ (x3 & x5)
        ^ (x3 & x6)
        ^ (x4 & x5)
        ^ (x4 & x6)
        ^ (x0 & x4)
        ^ x0
}

fn f5(x6: u32, x5: u32, x4: u32, x3: u32, x2: u32, x1: u32, x0: u32) -> u32 {
    (x1 & x4) ^ (x2 & x5) ^ (x3 & x6) ^ (x0 & x1 & x2 & x3) ^ (x0 & x5) ^ x0
}

fn fold_output(state: &mut [u32; 8], output_bits: usize) {
    match output_bits {
        128 => {
            state[3] = state[3].wrapping_add(
                (state[7] & 0xff00_0000)
                    | (state[6] & 0x00ff_0000)
                    | (state[5] & 0x0000_ff00)
                    | (state[4] & 0x0000_00ff),
            );
            state[2] = state[2].wrapping_add(
                (((state[7] & 0x00ff_0000) | (state[6] & 0x0000_ff00) | (state[5] & 0x0000_00ff))
                    << 8)
                    | ((state[4] & 0xff00_0000) >> 24),
            );
            state[1] = state[1].wrapping_add(
                (((state[7] & 0x0000_ff00) | (state[6] & 0x0000_00ff)) << 16)
                    | (((state[5] & 0xff00_0000) | (state[4] & 0x00ff_0000)) >> 16),
            );
            state[0] = state[0].wrapping_add(
                ((state[7] & 0x0000_00ff) << 24)
                    | (((state[6] & 0xff00_0000)
                        | (state[5] & 0x00ff_0000)
                        | (state[4] & 0x0000_ff00))
                        >> 8),
            );
        }
        160 => {
            state[4] = state[4].wrapping_add(
                ((state[7] & 0xfe00_0000) | (state[6] & 0x01f8_0000) | (state[5] & 0x0007_f000))
                    >> 12,
            );
            state[3] = state[3].wrapping_add(
                ((state[7] & 0x01f8_0000) | (state[6] & 0x0007_f000) | (state[5] & 0x0000_0fc0))
                    >> 6,
            );
            state[2] = state[2].wrapping_add(
                (state[7] & 0x0007_f000) | (state[6] & 0x0000_0fc0) | (state[5] & 0x0000_003f),
            );
            state[1] = state[1].wrapping_add(
                ((state[7] & 0x0000_0fc0) | (state[6] & 0x0000_003f) | (state[5] & 0xfe00_0000))
                    .rotate_right(25),
            );
            state[0] = state[0].wrapping_add(
                ((state[7] & 0x0000_003f) | (state[6] & 0xfe00_0000) | (state[5] & 0x01f8_0000))
                    .rotate_right(19),
            );
        }
        192 => {
            state[5] =
                state[5].wrapping_add(((state[7] & 0xfc00_0000) | (state[6] & 0x03e0_0000)) >> 21);
            state[4] =
                state[4].wrapping_add(((state[7] & 0x03e0_0000) | (state[6] & 0x001f_0000)) >> 16);
            state[3] =
                state[3].wrapping_add(((state[7] & 0x001f_0000) | (state[6] & 0x0000_fc00)) >> 10);
            state[2] =
                state[2].wrapping_add(((state[7] & 0x0000_fc00) | (state[6] & 0x0000_03e0)) >> 5);
            state[1] = state[1].wrapping_add((state[7] & 0x0000_03e0) | (state[6] & 0x0000_001f));
            state[0] = state[0].wrapping_add(
                ((state[7] & 0x0000_001f) | (state[6] & 0xfc00_0000)).rotate_right(26),
            );
        }
        224 => {
            state[6] = state[6].wrapping_add(state[7] & 0x0000_000f);
            state[5] = state[5].wrapping_add((state[7] >> 4) & 0x0000_001f);
            state[4] = state[4].wrapping_add((state[7] >> 9) & 0x0000_000f);
            state[3] = state[3].wrapping_add((state[7] >> 13) & 0x0000_001f);
            state[2] = state[2].wrapping_add((state[7] >> 18) & 0x0000_000f);
            state[1] = state[1].wrapping_add((state[7] >> 22) & 0x0000_001f);
            state[0] = state[0].wrapping_add((state[7] >> 27) & 0x0000_001f);
        }
        256 => {}
        _ => unreachable!("validated HAVAL output width"),
    }
}

fn encode_state(state: &[u32; 8], bytes: usize) -> Vec<u8> {
    let mut digest = Vec::with_capacity(bytes);
    for word in state {
        digest.extend_from_slice(&word.to_le_bytes());
    }
    digest.truncate(bytes);
    digest
}
