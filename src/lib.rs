use crate::order_book::SlabIndex;

pub mod engine;
pub mod itch_parser;
pub mod order_book;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct OrderLookupInfo {
    pub slab_idx: SlabIndex,
    pub locate: u16,
}

impl OrderLookupInfo {
    pub const NULL: Self = Self {
        slab_idx: SlabIndex::NULL,
        locate: 0,
    };
}

#[derive(Default, Clone)]
pub struct GlobalMap {
    // index is the `order_ref_number`
    pub map: Vec<OrderLookupInfo>,
}

impl GlobalMap {
    /// Returns `(locate, slab_idx)` for a live order, or `None` if the id is
    /// unknown or the slot was already removed.
    #[inline]
    #[must_use]
    pub fn get(&self, order_ref: u64) -> Option<(u16, SlabIndex)> {
        let info = self.map.get(order_ref.as_idx())?;
        if info.slab_idx.is_null() {
            return None;
        }
        Some((info.locate, info.slab_idx))
    }

    #[inline]
    pub fn set(&mut self, order_ref: u64, locate: u16, slab_idx: SlabIndex) {
        let idx = order_ref.as_idx();
        if idx >= self.map.len() {
            self.map.resize(idx + 1, OrderLookupInfo::NULL);
        }
        self.map[idx] = OrderLookupInfo { slab_idx, locate };
    }

    #[inline]
    pub fn remove(&mut self, order_ref: u64) {
        if let Some(info) = self.map.get_mut(order_ref.as_idx()) {
            *info = OrderLookupInfo::NULL;
        }
    }

    #[inline]
    pub fn clear(&mut self) {
        self.map.clear();
    }
}

#[allow(clippy::wrong_self_convention)]
pub trait NumericConversions {
    fn as_idx(self) -> usize;
    fn as_u32(self) -> u32;
}

impl NumericConversions for u64 {
    #[inline]
    fn as_idx(self) -> usize {
        self.try_into().expect("Machine is 64-bit")
    }
    fn as_u32(self) -> u32 {
        unreachable!("as_u32 is not implemented for u64")
    }
}

impl NumericConversions for usize {
    fn as_idx(self) -> usize {
        self
    }
    #[inline]
    fn as_u32(self) -> u32 {
        self.try_into().expect("overflow")
    }
}
