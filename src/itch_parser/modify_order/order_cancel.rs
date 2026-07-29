#![allow(clippy::inline_always)]
use crate::itch_parser::{
    Message,
    types::{U16Be, U32Be, U48Be, U64Be},
};
use zerocopy::{FromBytes, Immutable, IntoBytes, KnownLayout, Unaligned};

#[repr(C)]
#[derive(Debug, Clone, Copy, FromBytes, IntoBytes, Immutable, KnownLayout, Unaligned)]
pub struct OrderCancel {
    pub message_type: u8,

    /// Locate code identifying the security
    pub stock_locate: U16Be,

    /// Nasdaq internal tracking number
    pub tracking_number: U16Be,

    /// nanoseconds since midnight
    pub timestamp: U48Be,

    /// The unique reference number assigned to the new order at the time of receipt
    pub order_ref_number: U64Be,

    /// The number of shares being removed from the display size of the order as a result of a cancellation
    pub cancelled_shares: U32Be,
}

const _: () = assert!(std::mem::size_of::<OrderCancel>() == 23);
const _: () = assert!(std::mem::align_of::<OrderCancel>() == 1);

impl Message for OrderCancel {
    #[inline(always)]
    fn new(raw_message: &[u8]) -> Option<&Self> {
        Self::ref_from_bytes(raw_message).ok()
    }
}
