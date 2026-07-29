#![allow(clippy::inline_always)]
use zerocopy::{FromBytes, Immutable, IntoBytes, KnownLayout, Unaligned};

#[repr(transparent)]
#[derive(Debug, Clone, Copy, FromBytes, IntoBytes, Immutable, KnownLayout, Unaligned)]
pub struct U16Be(pub [u8; 2]);

#[repr(transparent)]
#[derive(Debug, Clone, Copy, FromBytes, IntoBytes, Immutable, KnownLayout, Unaligned)]
pub struct U32Be(pub [u8; 4]);

#[repr(transparent)]
#[derive(Debug, Clone, Copy, FromBytes, IntoBytes, Immutable, KnownLayout, Unaligned)]
pub struct U48Be(pub [u8; 6]);

#[repr(transparent)]
#[derive(Debug, Clone, Copy, FromBytes, IntoBytes, Immutable, KnownLayout, Unaligned)]
pub struct U64Be(pub [u8; 8]);

impl U16Be {
    #[inline(always)]
    #[must_use]
    pub fn get(self) -> u16 {
        u16::from_be_bytes(self.0)
    }
}

impl U32Be {
    #[inline(always)]
    #[must_use]
    pub fn get(self) -> u32 {
        u32::from_be_bytes(self.0)
    }
}

impl U48Be {
    #[inline(always)]
    #[must_use]
    pub fn get(self) -> u64 {
        let b = self.0;
        u64::from_be_bytes([0, 0, b[0], b[1], b[2], b[3], b[4], b[5]])
    }
}

impl U64Be {
    #[inline(always)]
    #[must_use]
    pub fn get(self) -> u64 {
        u64::from_be_bytes(self.0)
    }
}
