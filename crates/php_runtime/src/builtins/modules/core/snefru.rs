use super::snefru_tables::SNEFRU_TABLES;

const SNEFRU_BLOCK_SIZE: usize = 32;

#[derive(Clone)]
struct SnefruContext {
    state: [u32; 16],
    buffer: [u8; SNEFRU_BLOCK_SIZE],
    length: usize,
    bit_len: u64,
}

impl SnefruContext {
    fn new() -> Self {
        Self {
            state: [0; 16],
            buffer: [0; SNEFRU_BLOCK_SIZE],
            length: 0,
            bit_len: 0,
        }
    }

    fn update(&mut self, mut input: &[u8]) {
        self.bit_len = self
            .bit_len
            .wrapping_add((input.len() as u64).wrapping_mul(8));

        if self.length != 0 {
            let needed = SNEFRU_BLOCK_SIZE - self.length;
            if input.len() < needed {
                self.buffer[self.length..self.length + input.len()].copy_from_slice(input);
                self.length += input.len();
                self.buffer[self.length..].fill(0);
                return;
            }

            self.buffer[self.length..].copy_from_slice(&input[..needed]);
            let block = self.buffer;
            self.transform(&block);
            self.length = 0;
            input = &input[needed..];
        }

        while input.len() >= SNEFRU_BLOCK_SIZE {
            let mut block = [0_u8; SNEFRU_BLOCK_SIZE];
            block.copy_from_slice(&input[..SNEFRU_BLOCK_SIZE]);
            self.transform(&block);
            input = &input[SNEFRU_BLOCK_SIZE..];
        }

        self.buffer[..input.len()].copy_from_slice(input);
        self.length = input.len();
        self.buffer[self.length..].fill(0);
    }

    fn finalize(mut self) -> [u8; 32] {
        if self.length != 0 {
            let block = self.buffer;
            self.transform(&block);
        }

        self.state[14] = (self.bit_len >> 32) as u32;
        self.state[15] = self.bit_len as u32;
        snefru_rounds(&mut self.state);

        let mut digest = [0_u8; 32];
        for (word, chunk) in self.state.iter().take(8).zip(digest.chunks_exact_mut(4)) {
            chunk.copy_from_slice(&word.to_be_bytes());
        }
        digest
    }

    fn transform(&mut self, input: &[u8; SNEFRU_BLOCK_SIZE]) {
        for (slot, chunk) in self.state[8..].iter_mut().zip(input.chunks_exact(4)) {
            let mut bytes = [0_u8; 4];
            bytes.copy_from_slice(chunk);
            *slot = u32::from_be_bytes(bytes);
        }
        snefru_rounds(&mut self.state);
        self.state[8..].fill(0);
    }
}

pub(super) fn snefru_digest(input: &[u8]) -> Vec<u8> {
    let mut context = SnefruContext::new();
    context.update(input);
    context.finalize().to_vec()
}

fn snefru_rounds(input: &mut [u32; 16]) {
    const SHIFTS: [u32; 4] = [16, 8, 16, 24];
    let mut block = *input;

    for index in 0..8 {
        let t0 = &SNEFRU_TABLES[2 * index];
        let t1 = &SNEFRU_TABLES[2 * index + 1];
        for shift in SHIFTS {
            snefru_round(&mut block, 15, 0, 1, t0);
            snefru_round(&mut block, 0, 1, 2, t0);
            snefru_round(&mut block, 1, 2, 3, t1);
            snefru_round(&mut block, 2, 3, 4, t1);
            snefru_round(&mut block, 3, 4, 5, t0);
            snefru_round(&mut block, 4, 5, 6, t0);
            snefru_round(&mut block, 5, 6, 7, t1);
            snefru_round(&mut block, 6, 7, 8, t1);
            snefru_round(&mut block, 7, 8, 9, t0);
            snefru_round(&mut block, 8, 9, 10, t0);
            snefru_round(&mut block, 9, 10, 11, t1);
            snefru_round(&mut block, 10, 11, 12, t1);
            snefru_round(&mut block, 11, 12, 13, t0);
            snefru_round(&mut block, 12, 13, 14, t0);
            snefru_round(&mut block, 13, 14, 15, t1);
            snefru_round(&mut block, 14, 15, 0, t1);

            for word in &mut block {
                *word = word.rotate_right(shift);
            }
        }
    }

    input[0] ^= block[15];
    input[1] ^= block[14];
    input[2] ^= block[13];
    input[3] ^= block[12];
    input[4] ^= block[11];
    input[5] ^= block[10];
    input[6] ^= block[9];
    input[7] ^= block[8];
}

fn snefru_round(
    block: &mut [u32; 16],
    left: usize,
    current: usize,
    next: usize,
    table: &[u32; 256],
) {
    let value = table[(block[current] & 0xff) as usize];
    block[left] ^= value;
    block[next] ^= value;
}
