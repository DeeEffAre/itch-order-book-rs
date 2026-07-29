#![allow(clippy::inline_always)]
use crate::itch_parser::{
    Message,
    types::{U16Be, U32Be, U48Be, U64Be},
};
use zerocopy::{FromBytes, Immutable, IntoBytes, KnownLayout, Unaligned};

#[repr(C)]
#[derive(Debug, Clone, Copy, FromBytes, IntoBytes, Immutable, KnownLayout, Unaligned)]
pub struct OrderReplace {
    pub message_type: u8,

    /// Locate code identifying the security
    pub stock_locate: U16Be,

    /// Nasdaq internal tracking number
    pub tracking_number: U16Be,

    /// nanoseconds since midnight
    pub timestamp: U48Be,

    /// The original order reference number of the order being replaced
    pub org_order_ref_number: U64Be,

    /// The new reference number for this order at time of replacement
    // > Please note that the Nasdaq system will use this new order reference number for all subsequent updates
    pub new_order_ref_number: U64Be,

    /// The new total displayed quantity
    pub shares: U32Be,

    /// The new display price for the order.
    pub price: U32Be,
}

const _: () = assert!(std::mem::size_of::<OrderReplace>() == 35);
const _: () = assert!(std::mem::align_of::<OrderReplace>() == 1);

impl Message for OrderReplace {
    #[inline(always)]
    fn new(raw_message: &[u8]) -> Option<&Self> {
        Self::ref_from_bytes(raw_message).ok()
    }
}
