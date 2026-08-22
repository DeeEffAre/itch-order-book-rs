#![allow(clippy::inline_always)]

use crate::order_book::MAX_PRICE;

const BITSET_WORDS: usize = MAX_PRICE.div_ceil(64);
const L2_WORDS: usize = BITSET_WORDS.div_ceil(64);

// 2 level bitset
#[derive(Clone)]
pub struct PriceBitset {
    pub l1: [u64; BITSET_WORDS],
    pub l2: [u64; L2_WORDS],
}

impl Default for PriceBitset {
    fn default() -> Self {
        Self {
            l1: [0; BITSET_WORDS],
            l2: [0; L2_WORDS],
        }
    }
}

impl PriceBitset {
    #[inline(always)]
    pub fn set(&mut self, idx: usize) {
        let word_idx = idx >> 6;
        let bit_idx = idx & 63;
        self.l1[word_idx] |= 1u64 << bit_idx;

        let l2_word_idx = word_idx >> 6;
        let l2_bit_idx = word_idx & 63;
        self.l2[l2_word_idx] |= 1u64 << l2_bit_idx;
    }

    #[inline(always)]
    pub fn clear(&mut self, idx: usize) {
        let word_idx = idx >> 6;
        let bit_idx = idx & 63;
        self.l1[word_idx] &= !(1u64 << bit_idx);

        if self.l1[word_idx] == 0 {
            let l2_word_idx = word_idx >> 6;
            let l2_bit_idx = word_idx & 63;
            self.l2[l2_word_idx] &= !(1u64 << l2_bit_idx);
        }
    }

    /// finds the first set bit >= `from_idx`
    #[inline(always)]
    pub fn find_first_set_from(&self, from_idx: usize) -> Option<usize> {
        if from_idx >= MAX_PRICE {
            return None;
        }

        let mut word_idx = from_idx >> 6;
        let bit_idx = from_idx & 63;

        let masked = self.l1[word_idx] & (!0u64 << bit_idx);
        if masked != 0 {
            let res = (word_idx << 6) + masked.trailing_zeros() as usize;
            return if res < MAX_PRICE { Some(res) } else { None };
        }

        word_idx += 1;
        if word_idx >= BITSET_WORDS {
            return None;
        }

        let mut l2_word_idx = word_idx >> 6;
        let l2_bit_idx = word_idx & 63;

        let l2_masked = self.l2[l2_word_idx] & (!0u64 << l2_bit_idx);
        if l2_masked != 0 {
            word_idx = (l2_word_idx << 6) + l2_masked.trailing_zeros() as usize;
            let res = (word_idx << 6) + self.l1[word_idx].trailing_zeros() as usize;
            return if res < MAX_PRICE { Some(res) } else { None };
        }

        l2_word_idx += 1;
        while l2_word_idx < L2_WORDS {
            let l2_val = self.l2[l2_word_idx];
            if l2_val != 0 {
                word_idx = (l2_word_idx << 6) + l2_val.trailing_zeros() as usize;
                let res = (word_idx << 6) + self.l1[word_idx].trailing_zeros() as usize;
                return if res < MAX_PRICE { Some(res) } else { None };
            }
            l2_word_idx += 1;
        }

        None
    }

    /// finds the last set bit <= `from_idx`
    #[inline(always)]
    pub fn find_last_set_from(&self, from_idx: usize) -> Option<usize> {
        let from_idx = from_idx.min(MAX_PRICE - 1);

        let mut word_idx = from_idx >> 6;
        let bit_idx = from_idx & 63;

        let mask = if bit_idx == 63 {
            !0u64
        } else {
            (1u64 << (bit_idx + 1)) - 1
        };
        let masked = self.l1[word_idx] & mask;
        if masked != 0 {
            return Some((word_idx << 6) + 63 - masked.leading_zeros() as usize);
        }

        if word_idx == 0 {
            return None;
        }
        word_idx -= 1;

        let mut l2_word_idx = word_idx >> 6;
        let l2_bit_idx = word_idx & 63;

        let l2_mask = if l2_bit_idx == 63 {
            !0u64
        } else {
            (1u64 << (l2_bit_idx + 1)) - 1
        };
        let l2_masked = self.l2[l2_word_idx] & l2_mask;
        if l2_masked != 0 {
            word_idx = (l2_word_idx << 6) + 63 - l2_masked.leading_zeros() as usize;
            return Some((word_idx << 6) + 63 - self.l1[word_idx].leading_zeros() as usize);
        }

        while l2_word_idx > 0 {
            l2_word_idx -= 1;
            let l2_val = self.l2[l2_word_idx];
            if l2_val != 0 {
                word_idx = (l2_word_idx << 6) + 63 - l2_val.leading_zeros() as usize;
                return Some((word_idx << 6) + 63 - self.l1[word_idx].leading_zeros() as usize);
            }
        }

        None
    }

    /// shifts all bits to the right
    pub fn shift_right(&mut self, shift: usize) {
        if shift >= MAX_PRICE {
            *self = Self::default();
            return;
        }

        let word_shift = shift >> 6;
        let bit_shift = shift & 63;

        let mut new_l1 = [0u64; BITSET_WORDS];

        for (i, &src) in self.l1.iter().enumerate() {
            if src == 0 {
                continue;
            }

            let dst_idx = i + word_shift;
            if dst_idx < BITSET_WORDS {
                new_l1[dst_idx] |= src << bit_shift;
            }
            if bit_shift > 0 && dst_idx + 1 < BITSET_WORDS {
                new_l1[dst_idx + 1] |= src >> (64 - bit_shift);
            }
        }

        self.l1 = new_l1;

        // rebuild l2
        self.l2 = [0u64; L2_WORDS];
        for i in 0..BITSET_WORDS {
            if self.l1[i] != 0 {
                self.l2[i >> 6] |= 1u64 << (i & 63);
            }
        }
    }
}
