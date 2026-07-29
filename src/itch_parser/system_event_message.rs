#![allow(clippy::inline_always)]
use crate::itch_parser::{
    Message,
    types::{U16Be, U48Be},
};
use zerocopy::{FromBytes, Immutable, IntoBytes, KnownLayout, Unaligned};

#[repr(C)]
#[derive(Debug, Clone, Copy, FromBytes, IntoBytes, Immutable, KnownLayout, Unaligned)]
pub struct SystemEvent {
    pub message_type: u8,

    /// Locate code identifying the security
    pub stock_locate: U16Be,

    /// Nasdaq internal tracking number
    pub tracking_number: U16Be,

    /// nanoseconds since midnight
    pub timestamp: U48Be,

    pub event_code: u8,
}

const _: () = assert!(std::mem::size_of::<SystemEvent>() == 12);
const _: () = assert!(std::mem::align_of::<SystemEvent>() == 1);

impl Message for SystemEvent {
    #[inline(always)]
    fn new(raw_message: &[u8]) -> Option<&Self> {
        Self::ref_from_bytes(raw_message).ok()
    }
}
