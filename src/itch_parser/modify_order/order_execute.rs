#![allow(clippy::inline_always)]
use crate::itch_parser::{
    Message,
    types::{U16Be, U32Be, U48Be, U64Be},
};
use zerocopy::{FromBytes, Immutable, IntoBytes, KnownLayout, Unaligned};

#[repr(C)]
#[derive(Debug, Clone, Copy, FromBytes, IntoBytes, Immutable, KnownLayout, Unaligned)]
pub struct OrderExecute {
    pub message_type: u8,

    /// Locate code identifying the security
    pub stock_locate: U16Be,

    /// Nasdaq internal tracking number
    pub tracking_number: U16Be,

    /// nanoseconds since midnight
    pub timestamp: U48Be,

    /// The unique reference number assigned to the new order at the time of receipt
    pub order_ref_number: U64Be,

    /// The number of shares executed
    pub executed_shares: U32Be,

    /// The Nasdaq generated day unique Match Number of this execution. The Match Number is also referenced in the Trade Break Message
    pub match_number: U64Be,
}

#[repr(C)]
#[derive(Debug, Clone, Copy, FromBytes, IntoBytes, Immutable, KnownLayout, Unaligned)]
pub struct OrderExecuteWithPrice {
    pub message_type: u8,

    /// Locate code identifying the security
    pub stock_locate: U16Be,

    /// Nasdaq internal tracking number
    pub tracking_number: U16Be,

    /// nanoseconds since midnight
    pub timestamp: U48Be,

    /// The unique reference number assigned to the new order at the time of receipt
    pub order_ref_number: U64Be,

    /// The number of shares executed
    pub executed_shares: U32Be,

    /// The Nasdaq generated day unique Match Number of this execution. The Match Number is also referenced in the Trade Break Message
    pub match_number: U64Be,

    /// Indicates if the execution should be reflected on time and sales displays and volume calculations (Printable | Non-Printable)
    pub printable: u8,

    /// The Price at which the order execution occurred.
    pub execution_price: U32Be,
}

const _: () = assert!(std::mem::size_of::<OrderExecute>() == 31);
const _: () = assert!(std::mem::size_of::<OrderExecuteWithPrice>() == 36);
const _: () = assert!(std::mem::align_of::<OrderExecute>() == 1);
const _: () = assert!(std::mem::align_of::<OrderExecuteWithPrice>() == 1);

impl Message for OrderExecute {
    #[inline(always)]
    fn new(raw_message: &[u8]) -> Option<&Self> {
        Self::ref_from_bytes(raw_message).ok()
    }
}

impl Message for OrderExecuteWithPrice {
    #[inline(always)]
    fn new(raw_message: &[u8]) -> Option<&Self> {
        Self::ref_from_bytes(raw_message).ok()
    }
}
