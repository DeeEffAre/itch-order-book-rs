#![allow(clippy::inline_always)]
use crate::itch_parser::{
    Message,
    types::{U16Be, U32Be, U48Be, U64Be},
};
use zerocopy::{FromBytes, Immutable, IntoBytes, KnownLayout, Unaligned};

#[repr(C)]
#[derive(Debug, Clone, Copy, FromBytes, IntoBytes, Immutable, KnownLayout, Unaligned)]
pub struct AddOrderNoMPID {
    pub message_type: u8,

    /// Locate code identifying the security
    pub stock_locate: U16Be,

    /// Nasdaq internal tracking number
    pub tracking_number: U16Be,

    /// nanoseconds since midnight
    pub timestamp: U48Be,

    /// The unique reference number assigned to the new order at the time of receipt
    pub order_ref_number: U64Be,

    /// The type order being added. (Buy | Sell)
    pub indicator: u8,

    /// The total number of shares associated with the order being added to the book.
    pub shares: U32Be,

    /// Stock symbol, right padded with spaces
    pub stock: U64Be,

    /// The display price of the new order.
    pub price: U32Be,
}

#[repr(C)]
#[derive(Debug, Clone, Copy, FromBytes, IntoBytes, Immutable, KnownLayout, Unaligned)]
pub struct AddOrderWithMPID {
    pub message_type: u8,

    /// Locate code identifying the security
    pub stock_locate: U16Be,

    /// Nasdaq internal tracking number
    pub tracking_number: U16Be,

    /// nanoseconds since midnight
    pub timestamp: U48Be,

    /// The unique reference number assigned to the new order at the time of receipt
    pub order_ref_number: U64Be,

    /// The type order being added. (Buy | Sell)
    pub indicator: u8,

    /// The total number of shares associated with the order being added to the book.
    pub shares: U32Be,

    /// Stock symbol, right padded with spaces
    pub stock: U64Be,

    /// The display price of the new order.
    pub price: U32Be,

    /// Nasdaq Market participant identifier associated with the entered order
    pub attribution: U32Be,
}

const _: () = assert!(std::mem::size_of::<AddOrderNoMPID>() == 36);
const _: () = assert!(std::mem::size_of::<AddOrderWithMPID>() == 40);
const _: () = assert!(std::mem::align_of::<AddOrderNoMPID>() == 1);
const _: () = assert!(std::mem::align_of::<AddOrderWithMPID>() == 1);

impl Message for AddOrderNoMPID {
    #[inline(always)]
    fn new(raw_message: &[u8]) -> Option<&Self> {
        Self::ref_from_bytes(raw_message).ok()
    }
}

impl Message for AddOrderWithMPID {
    #[inline(always)]
    fn new(raw_message: &[u8]) -> Option<&Self> {
        Self::ref_from_bytes(raw_message).ok()
    }
}
