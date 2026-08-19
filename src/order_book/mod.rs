use std::ops::{Index, IndexMut};

use rustc_hash::FxHashMap;

use crate::itch_parser::{
    add_order::{AddOrderNoMPID, AddOrderWithMPID},
    handler::MessageHandler,
    modify_order::{
        order_cancel::OrderCancel,
        order_delete::OrderDelete,
        order_execute::{OrderExecute, OrderExecuteWithPrice},
        order_replace::OrderReplace,
    },
    system_event_message::SystemEvent,
};

mod order;

const SEGMENT_SIZE: usize = 1_000_000;
const MAX_SEGMENTS: usize = 2000;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct SlabIndex(u32);

impl SlabIndex {
    pub const NULL: Self = Self(u32::MAX);

    #[inline]
    #[must_use]
    pub fn is_null(self) -> bool {
        self.0 == u32::MAX
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Volume(u64);

impl std::ops::AddAssign<u32> for Volume {
    #[inline]
    fn add_assign(&mut self, rhs: u32) {
        self.0 += u64::from(rhs);
    }
}

impl std::ops::SubAssign<u32> for Volume {
    #[inline]
    fn sub_assign(&mut self, rhs: u32) {
        self.0 -= u64::from(rhs);
    }
}

#[derive(Debug, Clone, Copy)]
pub struct PriceLevel {
    // oldest order
    pub head: SlabIndex,
    // newest order
    pub tail: SlabIndex,
    // share volume
    pub volume: Volume,
    // how many orders in that price
    pub count: u32,
}

impl Default for PriceLevel {
    fn default() -> Self {
        Self {
            head: SlabIndex::NULL,
            tail: SlabIndex::NULL,
            volume: Volume::default(),
            count: 0,
        }
    }
}

#[derive(Default)]
pub struct Slab {
    orders: Vec<order::Order>,
    free_list: Vec<SlabIndex>,
}

impl Index<SlabIndex> for Slab {
    type Output = order::Order;

    #[inline]
    fn index(&self, index: SlabIndex) -> &Self::Output {
        debug_assert!(!index.is_null(), "Attempted to index NULL SlabIndex");
        &self.orders[index.0 as usize]
    }
}

impl IndexMut<SlabIndex> for Slab {
    #[inline]
    fn index_mut(&mut self, index: SlabIndex) -> &mut Self::Output {
        debug_assert!(
            !index.is_null(),
            "Attempted to mutably index NULL SlabIndex"
        );
        &mut self.orders[index.0 as usize]
    }
}

impl Slab {
    pub fn insert(&mut self, order: order::Order) -> SlabIndex {
        if let Some(idx) = self.free_list.pop() {
            self.orders[idx.0 as usize] = order;
            idx
        } else {
            let idx = self.orders.len() as u32;
            self.orders.push(order);
            SlabIndex(idx)
        }
    }

    pub fn remove(&mut self, idx: SlabIndex) {
        self.free_list.push(idx);
    }

    #[must_use]
    pub fn get(&self, idx: SlabIndex) -> &order::Order {
        &self.orders[idx.0 as usize]
    }

    pub fn get_mut(&mut self, idx: SlabIndex) -> &mut order::Order {
        &mut self.orders[idx.0 as usize]
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.orders.len() == self.free_list.len()
    }

    pub fn clear(&mut self) {
        self.orders.clear();
        self.free_list.clear();
    }
}

// How does that segment implementation work:
// Price -> 14,999,500
// segment_idx = 14,999,500 / 1,000,000 = 14
// offset = 14,999,500 % 1,000,000 = 999,500
// if `segment[14]` does not exists then allocate it (24MB)
// then access `segment[14][994,500]`

pub struct OrderBook {
    pub segments_bids: Vec<Option<Box<[PriceLevel]>>>,
    pub segments_asks: Vec<Option<Box<[PriceLevel]>>>,
    pub id_map: FxHashMap<order::Id, SlabIndex>,
    pub order_pool: Slab,
    pub best_bid: u64,
    pub best_ask: u64,
}

impl Default for OrderBook {
    fn default() -> Self {
        Self {
            segments_bids: Vec::new(),
            segments_asks: Vec::new(),
            id_map: FxHashMap::default(),
            order_pool: Slab::default(),
            best_bid: 0,
            best_ask: u64::MAX,
        }
    }
}

impl OrderBook {
    #[inline]
    #[must_use]
    pub fn get_order(&self, id: order::Id) -> Option<&order::Order> {
        self.id_map.get(&id).map(|&idx| &self.order_pool[idx])
    }

    #[inline]
    fn get_level(&self, side: order::Side, price: u64) -> Option<&PriceLevel> {
        let segment_idx = (price / SEGMENT_SIZE as u64) as usize;
        let offset = (price % SEGMENT_SIZE as u64) as usize;

        let segments = if side == order::Side::Buy {
            &self.segments_bids
        } else {
            &self.segments_asks
        };

        segments.get(segment_idx)?.as_ref().map(|seg| &seg[offset])
    }

    #[inline]
    #[must_use]
    pub fn get_price_level(&self, side: order::Side, price: order::Price) -> Option<&PriceLevel> {
        self.get_level(side, price.0)
    }

    fn add_order(&mut self, order: order::Order) {
        let price = order.price.0;
        let side = order.side;

        let new_order_slab_index = self.order_pool.insert(order);
        self.id_map.insert(order.id, new_order_slab_index);

        let segment_idx = (price / SEGMENT_SIZE as u64) as usize;
        let offset = (price % SEGMENT_SIZE as u64) as usize;

        assert!(
            segment_idx < MAX_SEGMENTS,
            "Price segment exceeds MAX_SEGMENTS"
        );

        let segments = if side == order::Side::Buy {
            if price > self.best_bid {
                self.best_bid = price;
            }
            &mut self.segments_bids
        } else {
            if price < self.best_ask {
                self.best_ask = price;
            }
            &mut self.segments_asks
        };

        if segment_idx >= segments.len() {
            segments.resize_with(segment_idx + 1, || None);
        }

        if segments[segment_idx].is_none() {
            segments[segment_idx] =
                Some(vec![PriceLevel::default(); SEGMENT_SIZE].into_boxed_slice());
        }

        let level = &mut segments[segment_idx].as_mut().unwrap()[offset];

        if level.tail.is_null() {
            level.head = new_order_slab_index;
            level.tail = new_order_slab_index;
        } else {
            let old_tail_slab_index = level.tail;
            self.order_pool[old_tail_slab_index].next = new_order_slab_index;
            self.order_pool[new_order_slab_index].prev = old_tail_slab_index;
            level.tail = new_order_slab_index;
        }
        level.volume += order.shares.0;
        level.count += 1;
    }

    fn remove_order_by_id(&mut self, order_id: order::Id) {
        let Some(&slab_idx) = self.id_map.get(&order_id) else {
            return;
        };
        let order_copy = self.order_pool[slab_idx];

        let price = order_copy.price.0;
        let side = order_copy.side;

        let segment_idx = (price / SEGMENT_SIZE as u64) as usize;
        let offset = (price % SEGMENT_SIZE as u64) as usize;

        let segments = if side == order::Side::Buy {
            &mut self.segments_bids
        } else {
            &mut self.segments_asks
        };

        let level = &mut segments[segment_idx].as_mut().unwrap()[offset];
        level.volume -= order_copy.shares.0;

        self.id_map.remove(&order_id);
        self.order_pool.remove(slab_idx);

        let prev = order_copy.prev;
        let next = order_copy.next;
        if prev.is_null() {
            level.head = next;
        } else {
            self.order_pool[prev].next = next;
        }
        if next.is_null() {
            level.tail = prev;
        } else {
            self.order_pool[next].prev = prev;
        }
        level.count -= 1;

        if level.count == 0 {
            if side == order::Side::Buy && price == self.best_bid {
                self.update_best_bid();
            } else if side == order::Side::Sell && price == self.best_ask {
                self.update_best_ask();
            }
        }
    }

    fn update_best_bid(&mut self) {
        let mut p = self.best_bid;
        while p > 0 {
            p -= 1;
            if let Some(level) = self.get_level(order::Side::Buy, p)
                && level.count > 0
            {
                self.best_bid = p;
                return;
            }
        }
        self.best_bid = 0;
    }

    fn update_best_ask(&mut self) {
        // no segment exists at or above this price, so the walk is bounded
        let limit = (self.segments_asks.len() as u64).saturating_mul(SEGMENT_SIZE as u64);
        let mut p = self.best_ask;
        while p < limit {
            p += 1;
            if let Some(level) = self.get_level(order::Side::Sell, p)
                && level.count > 0
            {
                self.best_ask = p;
                return;
            }
        }
        self.best_ask = u64::MAX;
    }

    fn handle_execution(&mut self, order_id: order::Id, executed_shares: u32) {
        let Some(&slab_idx) = self.id_map.get(&order_id) else {
            return;
        };

        let mut order_copy = self.order_pool[slab_idx];
        order_copy.shares -= executed_shares;
        let fully_done = order_copy.shares.0 == 0;

        let price = order_copy.price.0;
        let side = order_copy.side;

        let segment_idx = (price / SEGMENT_SIZE as u64) as usize;
        let offset = (price % SEGMENT_SIZE as u64) as usize;

        let segments = if side == order::Side::Buy {
            &mut self.segments_bids
        } else {
            &mut self.segments_asks
        };

        let level = &mut segments[segment_idx].as_mut().unwrap()[offset];
        level.volume -= executed_shares;

        if fully_done {
            self.id_map.remove(&order_id);
            self.order_pool.remove(slab_idx);

            let prev = order_copy.prev;
            let next = order_copy.next;
            if prev.is_null() {
                level.head = next;
            } else {
                self.order_pool[prev].next = next;
            }
            if next.is_null() {
                level.tail = prev;
            } else {
                self.order_pool[next].prev = prev;
            }
            level.count -= 1;

            if level.count == 0 {
                if side == order::Side::Buy && price == self.best_bid {
                    self.update_best_bid();
                } else if side == order::Side::Sell && price == self.best_ask {
                    self.update_best_ask();
                }
            }
        } else {
            self.order_pool[slab_idx] = order_copy;
        }
    }

    #[inline]
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.order_pool.is_empty()
    }

    #[inline]
    fn clear_all(&mut self) {
        self.segments_bids.clear();
        self.segments_asks.clear();
        self.id_map.clear();
        self.order_pool.clear();
        self.best_bid = 0;
        self.best_ask = u64::MAX;
    }
}

impl MessageHandler for OrderBook {
    fn on_add_order_no_mpid(&mut self, msg: &AddOrderNoMPID) {
        let order = order::Order::try_from(msg).expect("Invalid add order format");
        self.add_order(order);
    }

    fn on_add_order_with_mpid(&mut self, msg: &AddOrderWithMPID) {
        let order = order::Order::try_from(msg).expect("Invalid add order format");
        self.add_order(order);
    }

    fn on_order_execute_with_price(&mut self, msg: &OrderExecuteWithPrice) {
        self.handle_execution(
            order::Id(msg.order_ref_number.get()),
            msg.executed_shares.get(),
        );
    }

    fn on_order_execute(&mut self, msg: &OrderExecute) {
        self.handle_execution(
            order::Id(msg.order_ref_number.get()),
            msg.executed_shares.get(),
        );
    }

    fn on_order_delete(&mut self, msg: &OrderDelete) {
        self.remove_order_by_id(order::Id(msg.order_ref_number.get()));
    }

    fn on_order_cancel(&mut self, msg: &OrderCancel) {
        self.handle_execution(
            order::Id(msg.order_ref_number.get()),
            msg.cancelled_shares.get(),
        );
    }

    fn on_order_replace(&mut self, msg: &OrderReplace) {
        let old_id = order::Id(msg.org_order_ref_number.get());
        let Some(&slab_idx) = self.id_map.get(&old_id) else {
            return;
        };

        let old_order = *self.order_pool.get(slab_idx);
        self.remove_order_by_id(old_id);

        let mut new_order = order::Order::try_from(msg).expect("Invalid replace order format");
        new_order.side = old_order.side;

        self.add_order(new_order);
    }

    fn on_system_event(&mut self, msg: &SystemEvent) {
        match msg.event_code {
            b'O' | b'S' | b'E' | b'C' if !self.is_empty() => {
                self.clear_all();
            }
            _ => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::itch_parser::Message;
    use crate::order_book::order::{Id, Price, Quantity, Side};

    fn build_add_order_no_mpid(
        order_ref_number: u64,
        indicator: u8,
        shares: u32,
        price: u32,
        timestamp: u64,
    ) -> [u8; 36] {
        let mut buf = [0u8; 36];
        buf[0] = b'A';
        buf[1..3].copy_from_slice(&100u16.to_be_bytes());
        buf[3..5].copy_from_slice(&200u16.to_be_bytes());
        buf[5..11].copy_from_slice(&timestamp.to_be_bytes()[2..8]);
        buf[11..19].copy_from_slice(&order_ref_number.to_be_bytes());
        buf[19] = indicator;
        buf[20..24].copy_from_slice(&shares.to_be_bytes());
        buf[24..32].copy_from_slice(b"AAPL    ");
        buf[32..36].copy_from_slice(&price.to_be_bytes());
        buf
    }

    fn build_add_order_with_mpid(
        order_ref_number: u64,
        indicator: u8,
        shares: u32,
        price: u32,
        timestamp: u64,
        attribution: u32,
    ) -> [u8; 40] {
        let mut buf = [0u8; 40];
        buf[0] = b'F';
        buf[1..3].copy_from_slice(&101u16.to_be_bytes());
        buf[3..5].copy_from_slice(&201u16.to_be_bytes());
        buf[5..11].copy_from_slice(&timestamp.to_be_bytes()[2..8]);
        buf[11..19].copy_from_slice(&order_ref_number.to_be_bytes());
        buf[19] = indicator;
        buf[20..24].copy_from_slice(&shares.to_be_bytes());
        buf[24..32].copy_from_slice(b"GOOGL   ");
        buf[32..36].copy_from_slice(&price.to_be_bytes());
        buf[36..40].copy_from_slice(&attribution.to_be_bytes());
        buf
    }

    fn build_order_execute(
        order_ref_number: u64,
        executed_shares: u32,
        timestamp: u64,
    ) -> [u8; 31] {
        let mut buf = [0u8; 31];
        buf[0] = b'E';
        buf[1..3].copy_from_slice(&100u16.to_be_bytes());
        buf[3..5].copy_from_slice(&200u16.to_be_bytes());
        buf[5..11].copy_from_slice(&timestamp.to_be_bytes()[2..8]);
        buf[11..19].copy_from_slice(&order_ref_number.to_be_bytes());
        buf[19..23].copy_from_slice(&executed_shares.to_be_bytes());
        buf[23..31].copy_from_slice(&9999u64.to_be_bytes());
        buf
    }

    fn build_order_execute_with_price(
        order_ref_number: u64,
        executed_shares: u32,
        timestamp: u64,
        execution_price: u32,
    ) -> [u8; 36] {
        let mut buf = [0u8; 36];
        buf[0] = b'C';
        buf[1..3].copy_from_slice(&100u16.to_be_bytes());
        buf[3..5].copy_from_slice(&200u16.to_be_bytes());
        buf[5..11].copy_from_slice(&timestamp.to_be_bytes()[2..8]);
        buf[11..19].copy_from_slice(&order_ref_number.to_be_bytes());
        buf[19..23].copy_from_slice(&executed_shares.to_be_bytes());
        buf[23..31].copy_from_slice(&8888u64.to_be_bytes());
        buf[31] = b'Y';
        buf[32..36].copy_from_slice(&execution_price.to_be_bytes());
        buf
    }

    fn build_order_delete(order_ref_number: u64, timestamp: u64) -> [u8; 19] {
        let mut buf = [0u8; 19];
        buf[0] = b'D';
        buf[1..3].copy_from_slice(&100u16.to_be_bytes());
        buf[3..5].copy_from_slice(&200u16.to_be_bytes());
        buf[5..11].copy_from_slice(&timestamp.to_be_bytes()[2..8]);
        buf[11..19].copy_from_slice(&order_ref_number.to_be_bytes());
        buf
    }

    fn build_order_cancel(
        order_ref_number: u64,
        cancelled_shares: u32,
        timestamp: u64,
    ) -> [u8; 23] {
        let mut buf = [0u8; 23];
        buf[0] = b'X';
        buf[1..3].copy_from_slice(&100u16.to_be_bytes());
        buf[3..5].copy_from_slice(&200u16.to_be_bytes());
        buf[5..11].copy_from_slice(&timestamp.to_be_bytes()[2..8]);
        buf[11..19].copy_from_slice(&order_ref_number.to_be_bytes());
        buf[19..23].copy_from_slice(&cancelled_shares.to_be_bytes());
        buf
    }

    fn build_order_replace(
        org_order_ref_number: u64,
        new_order_ref_number: u64,
        shares: u32,
        price: u32,
        timestamp: u64,
    ) -> [u8; 35] {
        let mut buf = [0u8; 35];
        buf[0] = b'U';
        buf[1..3].copy_from_slice(&100u16.to_be_bytes());
        buf[3..5].copy_from_slice(&200u16.to_be_bytes());
        buf[5..11].copy_from_slice(&timestamp.to_be_bytes()[2..8]);
        buf[11..19].copy_from_slice(&org_order_ref_number.to_be_bytes());
        buf[19..27].copy_from_slice(&new_order_ref_number.to_be_bytes());
        buf[27..31].copy_from_slice(&shares.to_be_bytes());
        buf[31..35].copy_from_slice(&price.to_be_bytes());
        buf
    }

    fn build_system_event(event_code: u8, timestamp: u64) -> [u8; 12] {
        let mut buf = [0u8; 12];
        buf[0] = b'S';
        buf[1..3].copy_from_slice(&100u16.to_be_bytes());
        buf[3..5].copy_from_slice(&200u16.to_be_bytes());
        buf[5..11].copy_from_slice(&timestamp.to_be_bytes()[2..8]);
        buf[11] = event_code;
        buf
    }

    fn level_ids(book: &OrderBook, side: Side, price: u32) -> Vec<Id> {
        let level = book.get_price_level(side, Price::from(price)).unwrap();
        let mut ids = Vec::with_capacity(level.count as usize);
        let mut idx = level.head;
        while !idx.is_null() {
            ids.push(book.order_pool[idx].id);
            idx = book.order_pool[idx].next;
        }
        ids
    }

    // ---- on_add_order_no_mpid ----

    #[test]
    fn add_buy_order_no_mpid() {
        const ID: u64 = 42;
        const SIDE: u8 = b'B';
        const SHARES: u32 = 100;
        const PRICE: u32 = 15000;
        const TS: u64 = 1_000_000;

        let mut ob = OrderBook::default();
        let buf = build_add_order_no_mpid(ID, SIDE, SHARES, PRICE, TS);
        let msg = AddOrderNoMPID::new(&buf).unwrap();

        ob.on_add_order_no_mpid(msg);

        let level = ob.get_price_level(Side::Buy, Price::from(PRICE)).unwrap();
        assert_eq!(level.count, 1);
        assert_eq!(level.volume.0, u64::from(SHARES));
        assert_eq!(level_ids(&ob, Side::Buy, PRICE), vec![Id(ID)]);
        let order = ob.get_order(Id(ID)).unwrap();
        assert_eq!(order.shares, Quantity(SHARES));
        assert_eq!(order.side, Side::Buy);
        assert_eq!(order.price, Price::from(PRICE));
        assert_eq!(ob.best_ask, u64::MAX, "no asks should be present");
    }

    #[test]
    fn add_sell_order_no_mpid() {
        const ID: u64 = 00;
        const SIDE: u8 = b'S';
        const SHARES: u32 = 200;
        const PRICE: u32 = 25000;
        const TS: u64 = 2_000_000;

        let mut ob = OrderBook::default();
        let buf = build_add_order_no_mpid(ID, SIDE, SHARES, PRICE, TS);
        let msg = AddOrderNoMPID::new(&buf).unwrap();

        ob.on_add_order_no_mpid(msg);

        let level = ob.get_price_level(Side::Sell, Price::from(PRICE)).unwrap();
        assert_eq!(level.count, 1);
        assert_eq!(level.volume.0, u64::from(SHARES));
        assert_eq!(level_ids(&ob, Side::Sell, PRICE), vec![Id(ID)]);
        let order = ob.get_order(Id(ID)).unwrap();
        assert_eq!(order.shares, Quantity(SHARES));
        assert_eq!(order.side, Side::Sell);
        assert_eq!(order.price, Price::from(PRICE));
        assert_eq!(ob.best_bid, 0, "no bids should be present");
    }

    #[test]
    fn multiple_buy_orders_same_price_no_mpid() {
        const ID_1: u64 = 1;
        const ID_2: u64 = 2;
        const SIDE: u8 = b'B';
        const SHARES_1: u32 = 50;
        const SHARES_2: u32 = 75;
        const PRICE: u32 = 10000;
        const TS_1: u64 = 100;
        const TS_2: u64 = 200;

        let mut ob = OrderBook::default();
        let buf1 = build_add_order_no_mpid(ID_1, SIDE, SHARES_1, PRICE, TS_1);
        let buf2 = build_add_order_no_mpid(ID_2, SIDE, SHARES_2, PRICE, TS_2);

        ob.on_add_order_no_mpid(AddOrderNoMPID::new(&buf1).unwrap());
        ob.on_add_order_no_mpid(AddOrderNoMPID::new(&buf2).unwrap());

        let level = ob.get_price_level(Side::Buy, Price::from(PRICE)).unwrap();
        assert_eq!(level.count, 2);
        assert_eq!(level.volume.0, u64::from(SHARES_1 + SHARES_2));
        assert_eq!(level_ids(&ob, Side::Buy, PRICE), vec![Id(ID_1), Id(ID_2)]);
    }

    #[test]
    fn multiple_sell_orders_same_price_no_mpid() {
        const ID_1: u64 = 3;
        const ID_2: u64 = 4;
        const SIDE: u8 = b'S';
        const SHARES_1: u32 = 30;
        const SHARES_2: u32 = 60;
        const PRICE: u32 = 5000;
        const TS_1: u64 = 300;
        const TS_2: u64 = 400;

        let mut ob = OrderBook::default();
        let buf1 = build_add_order_no_mpid(ID_1, SIDE, SHARES_1, PRICE, TS_1);
        let buf2 = build_add_order_no_mpid(ID_2, SIDE, SHARES_2, PRICE, TS_2);

        ob.on_add_order_no_mpid(AddOrderNoMPID::new(&buf1).unwrap());
        ob.on_add_order_no_mpid(AddOrderNoMPID::new(&buf2).unwrap());

        let level = ob.get_price_level(Side::Sell, Price::from(PRICE)).unwrap();
        assert_eq!(level.count, 2);
        assert_eq!(level.volume.0, u64::from(SHARES_1 + SHARES_2));
        assert_eq!(level_ids(&ob, Side::Sell, PRICE), vec![Id(ID_1), Id(ID_2)]);
    }

    #[test]
    fn bids_sorted_high_to_low_no_mpid() {
        const SIDE: u8 = b'B';
        const SHARES: u32 = 10;
        const PRICE_LOW: u32 = 10000;
        const PRICE_MID: u32 = 15000;
        const PRICE_HIGH: u32 = 20000;
        const TS_1: u64 = 1;
        const TS_2: u64 = 2;
        const TS_3: u64 = 3;

        let mut ob = OrderBook::default();
        let buf1 = build_add_order_no_mpid(10, SIDE, SHARES, PRICE_LOW, TS_1);
        let buf2 = build_add_order_no_mpid(11, SIDE, SHARES, PRICE_HIGH, TS_2);
        let buf3 = build_add_order_no_mpid(12, SIDE, SHARES, PRICE_MID, TS_3);

        ob.on_add_order_no_mpid(AddOrderNoMPID::new(&buf1).unwrap());
        ob.on_add_order_no_mpid(AddOrderNoMPID::new(&buf2).unwrap());
        ob.on_add_order_no_mpid(AddOrderNoMPID::new(&buf3).unwrap());

        assert_eq!(ob.best_bid, u64::from(PRICE_HIGH));
        assert_eq!(
            ob.get_price_level(Side::Buy, Price::from(PRICE_LOW))
                .unwrap()
                .count,
            1
        );
        assert_eq!(
            ob.get_price_level(Side::Buy, Price::from(PRICE_MID))
                .unwrap()
                .count,
            1
        );
        assert_eq!(
            ob.get_price_level(Side::Buy, Price::from(PRICE_HIGH))
                .unwrap()
                .count,
            1
        );
    }

    #[test]
    fn asks_sorted_low_to_high_no_mpid() {
        const SIDE: u8 = b'S';
        const SHARES: u32 = 10;
        const PRICE_LOW: u32 = 10000;
        const PRICE_MID: u32 = 15000;
        const PRICE_HIGH: u32 = 20000;
        const TS_1: u64 = 1;
        const TS_2: u64 = 2;
        const TS_3: u64 = 3;

        let mut ob = OrderBook::default();
        let buf1 = build_add_order_no_mpid(20, SIDE, SHARES, PRICE_HIGH, TS_1);
        let buf2 = build_add_order_no_mpid(21, SIDE, SHARES, PRICE_LOW, TS_2);
        let buf3 = build_add_order_no_mpid(22, SIDE, SHARES, PRICE_MID, TS_3);

        ob.on_add_order_no_mpid(AddOrderNoMPID::new(&buf1).unwrap());
        ob.on_add_order_no_mpid(AddOrderNoMPID::new(&buf2).unwrap());
        ob.on_add_order_no_mpid(AddOrderNoMPID::new(&buf3).unwrap());

        assert_eq!(ob.best_ask, u64::from(PRICE_LOW));
        assert_eq!(
            ob.get_price_level(Side::Sell, Price::from(PRICE_LOW))
                .unwrap()
                .count,
            1
        );
        assert_eq!(
            ob.get_price_level(Side::Sell, Price::from(PRICE_MID))
                .unwrap()
                .count,
            1
        );
        assert_eq!(
            ob.get_price_level(Side::Sell, Price::from(PRICE_HIGH))
                .unwrap()
                .count,
            1
        );
    }

    // ---- on_add_order_with_mpid ----

    #[test]
    fn add_buy_order_with_mpid() {
        const ID: u64 = 55;
        const SIDE: u8 = b'B';
        const SHARES: u32 = 300;
        const PRICE: u32 = 18000;
        const TS: u64 = 5_000_000;
        const ATTRIBUTION: u32 = 888;

        let mut ob = OrderBook::default();
        let buf = build_add_order_with_mpid(ID, SIDE, SHARES, PRICE, TS, ATTRIBUTION);
        let msg = AddOrderWithMPID::new(&buf).unwrap();

        ob.on_add_order_with_mpid(msg);

        let level = ob.get_price_level(Side::Buy, Price::from(PRICE)).unwrap();
        assert_eq!(level.count, 1);
        assert_eq!(level.volume.0, u64::from(SHARES));
        assert_eq!(level_ids(&ob, Side::Buy, PRICE), vec![Id(ID)]);
        let order = ob.get_order(Id(ID)).unwrap();
        assert_eq!(order.shares, Quantity(SHARES));
        assert_eq!(order.side, Side::Buy);
        assert_eq!(order.price, Price::from(PRICE));
        assert_eq!(ob.best_ask, u64::MAX, "no asks should be present");
    }

    #[test]
    fn add_sell_order_with_mpid() {
        const ID: u64 = 77;
        const SIDE: u8 = b'S';
        const SHARES: u32 = 150;
        const PRICE: u32 = 12000;
        const TS: u64 = 3_000_000;
        const ATTRIBUTION: u32 = 444;

        let mut ob = OrderBook::default();
        let buf = build_add_order_with_mpid(ID, SIDE, SHARES, PRICE, TS, ATTRIBUTION);
        let msg = AddOrderWithMPID::new(&buf).unwrap();

        ob.on_add_order_with_mpid(msg);

        let level = ob.get_price_level(Side::Sell, Price::from(PRICE)).unwrap();
        assert_eq!(level.count, 1);
        assert_eq!(level.volume.0, u64::from(SHARES));
        assert_eq!(level_ids(&ob, Side::Sell, PRICE), vec![Id(ID)]);
        let order = ob.get_order(Id(ID)).unwrap();
        assert_eq!(order.shares, Quantity(SHARES));
        assert_eq!(order.side, Side::Sell);
        assert_eq!(order.price, Price::from(PRICE));
        assert_eq!(ob.best_bid, 0, "no bids should be present");
    }

    #[test]
    fn mixed_buy_sell_with_mpid() {
        const BUY_ID: u64 = 1;
        const BUY_SHARES: u32 = 100;
        const BUY_PRICE: u32 = 10000;
        const BUY_TS: u64 = 111;
        const SELL_ID: u64 = 2;
        const SELL_SHARES: u32 = 50;
        const SELL_PRICE: u32 = 12000;
        const SELL_TS: u64 = 222;
        const ATTRIBUTION: u32 = 1;

        let mut ob = OrderBook::default();
        let buf_buy =
            build_add_order_with_mpid(BUY_ID, b'B', BUY_SHARES, BUY_PRICE, BUY_TS, ATTRIBUTION);
        let buf_sell =
            build_add_order_with_mpid(SELL_ID, b'S', SELL_SHARES, SELL_PRICE, SELL_TS, ATTRIBUTION);

        ob.on_add_order_with_mpid(AddOrderWithMPID::new(&buf_buy).unwrap());
        ob.on_add_order_with_mpid(AddOrderWithMPID::new(&buf_sell).unwrap());

        assert_eq!(
            ob.get_price_level(Side::Buy, Price::from(BUY_PRICE))
                .unwrap()
                .count,
            1
        );
        assert_eq!(
            ob.get_price_level(Side::Sell, Price::from(SELL_PRICE))
                .unwrap()
                .count,
            1
        );
        assert_eq!(level_ids(&ob, Side::Buy, BUY_PRICE), vec![Id(BUY_ID)]);
        assert_eq!(level_ids(&ob, Side::Sell, SELL_PRICE), vec![Id(SELL_ID)]);
        assert_eq!(ob.best_bid, u64::from(BUY_PRICE));
        assert_eq!(ob.best_ask, u64::from(SELL_PRICE));
    }

    #[test]
    fn empty_order_book() {
        let ob = OrderBook::default();
        assert!(ob.is_empty());
        assert_eq!(ob.best_bid, 0);
        assert_eq!(ob.best_ask, u64::MAX);
    }

    // ---- on_order_execute ----

    #[test]
    fn execute_full_buy_order() {
        const ID: u64 = 10;
        const PRICE: u32 = 10000;
        const SHARES: u32 = 200;

        let mut ob = OrderBook::default();
        ob.on_add_order_no_mpid(
            AddOrderNoMPID::new(&build_add_order_no_mpid(ID, b'B', SHARES, PRICE, 1)).unwrap(),
        );

        let buf = build_order_execute(ID, SHARES, 100);
        ob.on_order_execute(OrderExecute::new(&buf).unwrap());

        let level = ob.get_price_level(Side::Buy, Price::from(PRICE)).unwrap();
        assert_eq!(
            level.count, 0,
            "price level should be empty after last order executed"
        );
        assert!(
            ob.get_order(Id(ID)).is_none(),
            "order should be removed from pool"
        );
        assert_eq!(ob.best_bid, 0);
    }

    #[test]
    fn execute_partial_buy_order() {
        const ID: u64 = 11;
        const PRICE: u32 = 10000;
        const INITIAL_SHARES: u32 = 200;
        const EXECUTED: u32 = 80;

        let mut ob = OrderBook::default();
        ob.on_add_order_no_mpid(
            AddOrderNoMPID::new(&build_add_order_no_mpid(ID, b'B', INITIAL_SHARES, PRICE, 1))
                .unwrap(),
        );

        let buf = build_order_execute(ID, EXECUTED, 100);
        ob.on_order_execute(OrderExecute::new(&buf).unwrap());

        let level = ob.get_price_level(Side::Buy, Price::from(PRICE)).unwrap();
        assert_eq!(level.count, 1, "queue should still exist");
        assert_eq!(level.volume.0, u64::from(INITIAL_SHARES - EXECUTED));
        assert_eq!(level_ids(&ob, Side::Buy, PRICE), vec![Id(ID)]);
        let order = ob.get_order(Id(ID)).unwrap();
        assert_eq!(order.shares, Quantity(INITIAL_SHARES - EXECUTED));
    }

    #[test]
    fn execute_two_partials_become_full() {
        const ID: u64 = 12;
        const PRICE: u32 = 10000;
        const INITIAL_SHARES: u32 = 200;

        let mut ob = OrderBook::default();
        ob.on_add_order_no_mpid(
            AddOrderNoMPID::new(&build_add_order_no_mpid(ID, b'B', INITIAL_SHARES, PRICE, 1))
                .unwrap(),
        );

        ob.on_order_execute(OrderExecute::new(&build_order_execute(ID, 50, 100)).unwrap());
        ob.on_order_execute(
            OrderExecute::new(&build_order_execute(ID, INITIAL_SHARES - 50, 200)).unwrap(),
        );

        assert_eq!(
            ob.get_price_level(Side::Buy, Price::from(PRICE))
                .unwrap()
                .count,
            0
        );
        assert!(ob.get_order(Id(ID)).is_none());
        assert_eq!(ob.best_bid, 0);
    }

    #[test]
    fn execute_unknown_order_no_panic() {
        let mut ob = OrderBook::default();
        let buf = build_order_execute(999, 100, 1);
        ob.on_order_execute(OrderExecute::new(&buf).unwrap());
        // should not panic, nothing happens
    }

    #[test]
    fn execute_full_sell_order() {
        const ID: u64 = 20;
        const PRICE: u32 = 15000;
        const SHARES: u32 = 300;

        let mut ob = OrderBook::default();
        ob.on_add_order_no_mpid(
            AddOrderNoMPID::new(&build_add_order_no_mpid(ID, b'S', SHARES, PRICE, 1)).unwrap(),
        );

        let buf = build_order_execute(ID, SHARES, 100);
        ob.on_order_execute(OrderExecute::new(&buf).unwrap());

        assert_eq!(
            ob.get_price_level(Side::Sell, Price::from(PRICE))
                .unwrap()
                .count,
            0
        );
        assert!(ob.get_order(Id(ID)).is_none());
        assert_eq!(ob.best_ask, u64::MAX);
    }

    // ---- on_order_execute_with_price ----

    #[test]
    fn execute_with_price_full_buy_order() {
        const ID: u64 = 30;
        const PRICE: u32 = 10000;
        const SHARES: u32 = 100;

        let mut ob = OrderBook::default();
        ob.on_add_order_no_mpid(
            AddOrderNoMPID::new(&build_add_order_no_mpid(ID, b'B', SHARES, PRICE, 1)).unwrap(),
        );

        let buf = build_order_execute_with_price(ID, SHARES, 100, PRICE);
        ob.on_order_execute_with_price(OrderExecuteWithPrice::new(&buf).unwrap());

        assert_eq!(
            ob.get_price_level(Side::Buy, Price::from(PRICE))
                .unwrap()
                .count,
            0
        );
        assert!(ob.get_order(Id(ID)).is_none());
        assert_eq!(ob.best_bid, 0);
    }

    #[test]
    fn execute_with_price_partial_buy_order() {
        const ID: u64 = 31;
        const PRICE: u32 = 10000;
        const INITIAL_SHARES: u32 = 200;
        const EXECUTED: u32 = 75;

        let mut ob = OrderBook::default();
        ob.on_add_order_no_mpid(
            AddOrderNoMPID::new(&build_add_order_no_mpid(ID, b'B', INITIAL_SHARES, PRICE, 1))
                .unwrap(),
        );

        let buf = build_order_execute_with_price(ID, EXECUTED, 100, PRICE);
        ob.on_order_execute_with_price(OrderExecuteWithPrice::new(&buf).unwrap());

        let level = ob.get_price_level(Side::Buy, Price::from(PRICE)).unwrap();
        assert_eq!(level.count, 1);
        assert_eq!(level.volume.0, u64::from(INITIAL_SHARES - EXECUTED));
        assert_eq!(level_ids(&ob, Side::Buy, PRICE), vec![Id(ID)]);
        let order = ob.get_order(Id(ID)).unwrap();
        assert_eq!(order.shares, Quantity(INITIAL_SHARES - EXECUTED));
    }

    #[test]
    fn execute_with_price_unknown_order_no_panic() {
        let mut ob = OrderBook::default();
        let buf = build_order_execute_with_price(999, 100, 1, 10000);
        ob.on_order_execute_with_price(OrderExecuteWithPrice::new(&buf).unwrap());
    }

    // ---- on_order_delete ----

    #[test]
    fn delete_existing_buy_order() {
        const ID: u64 = 40;
        const PRICE: u32 = 10000;
        const SHARES: u32 = 100;

        let mut ob = OrderBook::default();
        ob.on_add_order_no_mpid(
            AddOrderNoMPID::new(&build_add_order_no_mpid(ID, b'B', SHARES, PRICE, 1)).unwrap(),
        );

        let buf = build_order_delete(ID, 100);
        ob.on_order_delete(OrderDelete::new(&buf).unwrap());

        assert_eq!(
            ob.get_price_level(Side::Buy, Price::from(PRICE))
                .unwrap()
                .count,
            0
        );
        assert!(ob.get_order(Id(ID)).is_none());
        assert_eq!(ob.best_bid, 0);
    }

    #[test]
    fn delete_one_of_two_at_same_price() {
        const ID_1: u64 = 50;
        const ID_2: u64 = 51;
        const PRICE: u32 = 10000;
        const SHARES: u32 = 100;

        let mut ob = OrderBook::default();
        ob.on_add_order_no_mpid(
            AddOrderNoMPID::new(&build_add_order_no_mpid(ID_1, b'B', SHARES, PRICE, 1)).unwrap(),
        );
        ob.on_add_order_no_mpid(
            AddOrderNoMPID::new(&build_add_order_no_mpid(ID_2, b'B', SHARES, PRICE, 2)).unwrap(),
        );

        ob.on_order_delete(OrderDelete::new(&build_order_delete(ID_1, 100)).unwrap());

        let level = ob.get_price_level(Side::Buy, Price::from(PRICE)).unwrap();
        assert_eq!(level.count, 1);
        assert_eq!(level_ids(&ob, Side::Buy, PRICE), vec![Id(ID_2)]);
        assert!(ob.get_order(Id(ID_1)).is_none());
        assert!(ob.get_order(Id(ID_2)).is_some());
        assert_eq!(ob.best_bid, u64::from(PRICE));
    }

    #[test]
    fn delete_unknown_order_no_panic() {
        let mut ob = OrderBook::default();
        let buf = build_order_delete(999, 1);
        ob.on_order_delete(OrderDelete::new(&buf).unwrap());
    }

    #[test]
    fn delete_existing_sell_order() {
        const ID: u64 = 60;
        const PRICE: u32 = 15000;
        const SHARES: u32 = 200;

        let mut ob = OrderBook::default();
        ob.on_add_order_no_mpid(
            AddOrderNoMPID::new(&build_add_order_no_mpid(ID, b'S', SHARES, PRICE, 1)).unwrap(),
        );

        let buf = build_order_delete(ID, 100);
        ob.on_order_delete(OrderDelete::new(&buf).unwrap());

        assert_eq!(
            ob.get_price_level(Side::Sell, Price::from(PRICE))
                .unwrap()
                .count,
            0
        );
        assert!(ob.get_order(Id(ID)).is_none());
        assert_eq!(ob.best_ask, u64::MAX);
    }

    // ---- on_order_cancel ----

    #[test]
    fn cancel_all_shares_removes_order() {
        const ID: u64 = 70;
        const PRICE: u32 = 10000;
        const SHARES: u32 = 500;

        let mut ob = OrderBook::default();
        ob.on_add_order_no_mpid(
            AddOrderNoMPID::new(&build_add_order_no_mpid(ID, b'B', SHARES, PRICE, 1)).unwrap(),
        );

        let buf = build_order_cancel(ID, SHARES, 100);
        ob.on_order_cancel(OrderCancel::new(&buf).unwrap());

        assert_eq!(
            ob.get_price_level(Side::Buy, Price::from(PRICE))
                .unwrap()
                .count,
            0
        );
        assert!(ob.get_order(Id(ID)).is_none());
        assert_eq!(ob.best_bid, 0);
    }

    #[test]
    fn cancel_partial_shares_keeps_order() {
        const ID: u64 = 71;
        const PRICE: u32 = 10000;
        const INITIAL_SHARES: u32 = 500;
        const CANCELLED: u32 = 200;

        let mut ob = OrderBook::default();
        ob.on_add_order_no_mpid(
            AddOrderNoMPID::new(&build_add_order_no_mpid(ID, b'B', INITIAL_SHARES, PRICE, 1))
                .unwrap(),
        );

        let buf = build_order_cancel(ID, CANCELLED, 100);
        ob.on_order_cancel(OrderCancel::new(&buf).unwrap());

        let level = ob.get_price_level(Side::Buy, Price::from(PRICE)).unwrap();
        assert_eq!(level.count, 1);
        assert_eq!(level.volume.0, u64::from(INITIAL_SHARES - CANCELLED));
        assert_eq!(level_ids(&ob, Side::Buy, PRICE), vec![Id(ID)]);
        let order = ob.get_order(Id(ID)).unwrap();
        assert_eq!(order.shares, Quantity(INITIAL_SHARES - CANCELLED));
    }

    #[test]
    fn cancel_two_partials_become_full() {
        const ID: u64 = 72;
        const PRICE: u32 = 10000;
        const INITIAL_SHARES: u32 = 400;

        let mut ob = OrderBook::default();
        ob.on_add_order_no_mpid(
            AddOrderNoMPID::new(&build_add_order_no_mpid(ID, b'B', INITIAL_SHARES, PRICE, 1))
                .unwrap(),
        );

        ob.on_order_cancel(OrderCancel::new(&build_order_cancel(ID, 150, 100)).unwrap());
        ob.on_order_cancel(
            OrderCancel::new(&build_order_cancel(ID, INITIAL_SHARES - 150, 200)).unwrap(),
        );

        assert_eq!(
            ob.get_price_level(Side::Buy, Price::from(PRICE))
                .unwrap()
                .count,
            0
        );
        assert!(ob.get_order(Id(ID)).is_none());
        assert_eq!(ob.best_bid, 0);
    }

    #[test]
    fn cancel_unknown_order_no_panic() {
        let mut ob = OrderBook::default();
        let buf = build_order_cancel(999, 100, 1);
        ob.on_order_cancel(OrderCancel::new(&buf).unwrap());
    }

    #[test]
    fn cancel_all_sell_order() {
        const ID: u64 = 80;
        const PRICE: u32 = 15000;
        const SHARES: u32 = 300;

        let mut ob = OrderBook::default();
        ob.on_add_order_no_mpid(
            AddOrderNoMPID::new(&build_add_order_no_mpid(ID, b'S', SHARES, PRICE, 1)).unwrap(),
        );

        let buf = build_order_cancel(ID, SHARES, 100);
        ob.on_order_cancel(OrderCancel::new(&buf).unwrap());

        assert_eq!(
            ob.get_price_level(Side::Sell, Price::from(PRICE))
                .unwrap()
                .count,
            0
        );
        assert!(ob.get_order(Id(ID)).is_none());
        assert_eq!(ob.best_ask, u64::MAX);
    }

    // ---- on_order_replace ----

    #[test]
    fn replace_buy_order_new_price_and_shares() {
        const OLD_ID: u64 = 90;
        const NEW_ID: u64 = 91;
        const OLD_PRICE: u32 = 10000;
        const NEW_PRICE: u32 = 12000;
        const OLD_SHARES: u32 = 100;
        const NEW_SHARES: u32 = 250;

        let mut ob = OrderBook::default();
        ob.on_add_order_no_mpid(
            AddOrderNoMPID::new(&build_add_order_no_mpid(
                OLD_ID, b'B', OLD_SHARES, OLD_PRICE, 1,
            ))
            .unwrap(),
        );

        let buf = build_order_replace(OLD_ID, NEW_ID, NEW_SHARES, NEW_PRICE, 200);
        ob.on_order_replace(OrderReplace::new(&buf).unwrap());

        // old price level gone
        assert_eq!(
            ob.get_price_level(Side::Buy, Price::from(OLD_PRICE))
                .unwrap()
                .count,
            0
        );
        assert!(ob.get_order(Id(OLD_ID)).is_none());

        // new order at new price level
        let level = ob
            .get_price_level(Side::Buy, Price::from(NEW_PRICE))
            .unwrap();
        assert_eq!(level.count, 1);
        assert_eq!(level_ids(&ob, Side::Buy, NEW_PRICE), vec![Id(NEW_ID)]);
        let new_order = ob.get_order(Id(NEW_ID)).unwrap();
        assert_eq!(new_order.shares, Quantity(NEW_SHARES));
        assert_eq!(new_order.side, Side::Buy);
        assert_eq!(new_order.price, Price::from(NEW_PRICE));
        assert_eq!(ob.best_bid, u64::from(NEW_PRICE));
    }

    #[test]
    fn replace_sell_order_same_price() {
        const OLD_ID: u64 = 100;
        const NEW_ID: u64 = 101;
        const PRICE: u32 = 15000;
        const OLD_SHARES: u32 = 100;
        const NEW_SHARES: u32 = 300;

        let mut ob = OrderBook::default();
        ob.on_add_order_no_mpid(
            AddOrderNoMPID::new(&build_add_order_no_mpid(OLD_ID, b'S', OLD_SHARES, PRICE, 1))
                .unwrap(),
        );

        // add another sell at same price so queue isn't empty after remove
        ob.on_add_order_no_mpid(
            AddOrderNoMPID::new(&build_add_order_no_mpid(102, b'S', 50, PRICE, 2)).unwrap(),
        );

        let buf = build_order_replace(OLD_ID, NEW_ID, NEW_SHARES, PRICE, 200);
        ob.on_order_replace(OrderReplace::new(&buf).unwrap());

        let level = ob.get_price_level(Side::Sell, Price::from(PRICE)).unwrap();
        assert_eq!(level.count, 2);
        assert_eq!(level_ids(&ob, Side::Sell, PRICE), vec![Id(102), Id(NEW_ID)]);

        assert!(ob.get_order(Id(OLD_ID)).is_none());
        assert!(ob.get_order(Id(NEW_ID)).is_some());
    }

    #[test]
    fn replace_unknown_order_no_panic() {
        let mut ob = OrderBook::default();
        let buf = build_order_replace(999, 1000, 100, 10000, 1);
        ob.on_order_replace(OrderReplace::new(&buf).unwrap());
    }

    #[test]
    fn replace_preserves_side() {
        const OLD_ID: u64 = 110;
        const NEW_ID: u64 = 111;
        const PRICE: u32 = 10000;

        let mut ob = OrderBook::default();
        ob.on_add_order_no_mpid(
            AddOrderNoMPID::new(&build_add_order_no_mpid(OLD_ID, b'S', 100, PRICE, 1)).unwrap(),
        );

        let buf = build_order_replace(OLD_ID, NEW_ID, 200, PRICE, 200);
        ob.on_order_replace(OrderReplace::new(&buf).unwrap());

        assert_eq!(
            ob.get_price_level(Side::Sell, Price::from(PRICE))
                .unwrap()
                .count,
            1
        );
        let new_order = ob.get_order(Id(NEW_ID)).unwrap();
        assert_eq!(new_order.side, Side::Sell);
    }

    // ---- on_system_event ----

    #[test]
    fn system_event_clears_non_empty_book() {
        let mut ob = OrderBook::default();
        ob.on_add_order_no_mpid(
            AddOrderNoMPID::new(&build_add_order_no_mpid(1, b'B', 100, 10000, 1)).unwrap(),
        );
        ob.on_add_order_no_mpid(
            AddOrderNoMPID::new(&build_add_order_no_mpid(2, b'S', 50, 12000, 2)).unwrap(),
        );
        assert!(!ob.is_empty());

        let buf = build_system_event(b'O', 100);
        ob.on_system_event(SystemEvent::new(&buf).unwrap());

        assert!(ob.is_empty());
        assert!(ob.id_map.is_empty());
        assert_eq!(ob.best_bid, 0);
        assert_eq!(ob.best_ask, u64::MAX);
    }

    #[test]
    fn system_event_start_of_system_hours_clears() {
        let mut ob = OrderBook::default();
        ob.on_add_order_no_mpid(
            AddOrderNoMPID::new(&build_add_order_no_mpid(1, b'B', 100, 10000, 1)).unwrap(),
        );

        let buf = build_system_event(b'S', 100);
        ob.on_system_event(SystemEvent::new(&buf).unwrap());

        assert!(ob.is_empty());
    }

    #[test]
    fn system_event_end_of_system_hours_clears() {
        let mut ob = OrderBook::default();
        ob.on_add_order_no_mpid(
            AddOrderNoMPID::new(&build_add_order_no_mpid(1, b'B', 100, 10000, 1)).unwrap(),
        );

        let buf = build_system_event(b'E', 100);
        ob.on_system_event(SystemEvent::new(&buf).unwrap());

        assert!(ob.is_empty());
    }

    #[test]
    fn system_event_end_of_messages_clears() {
        let mut ob = OrderBook::default();
        ob.on_add_order_no_mpid(
            AddOrderNoMPID::new(&build_add_order_no_mpid(1, b'B', 100, 10000, 1)).unwrap(),
        );

        let buf = build_system_event(b'C', 100);
        ob.on_system_event(SystemEvent::new(&buf).unwrap());

        assert!(ob.is_empty());
    }

    #[test]
    fn system_event_on_empty_book_no_panic() {
        let mut ob = OrderBook::default();
        assert!(ob.is_empty());

        let buf = build_system_event(b'O', 100);
        ob.on_system_event(SystemEvent::new(&buf).unwrap());

        assert!(ob.is_empty());
    }

    #[test]
    fn system_event_market_hours_does_not_clear() {
        let mut ob = OrderBook::default();
        ob.on_add_order_no_mpid(
            AddOrderNoMPID::new(&build_add_order_no_mpid(1, b'B', 100, 10000, 1)).unwrap(),
        );

        let buf = build_system_event(b'Q', 100);
        ob.on_system_event(SystemEvent::new(&buf).unwrap());

        assert!(!ob.is_empty());
        assert_eq!(
            ob.get_price_level(Side::Buy, Price::from(10000))
                .unwrap()
                .count,
            1
        );
    }

    #[test]
    fn system_event_end_of_market_hours_does_not_clear() {
        let mut ob = OrderBook::default();
        ob.on_add_order_no_mpid(
            AddOrderNoMPID::new(&build_add_order_no_mpid(1, b'B', 100, 10000, 1)).unwrap(),
        );

        let buf = build_system_event(b'M', 100);
        ob.on_system_event(SystemEvent::new(&buf).unwrap());

        assert!(!ob.is_empty());
    }

    // ---- segmented price mapping ----

    #[test]
    fn lazy_segment_allocation_only_touches_used_segments() {
        const ID: u64 = 1;
        const PRICE: u32 = 14_999_500; // segment 14, offset 999_500

        let mut ob = OrderBook::default();
        assert!(ob.segments_bids.is_empty());
        assert!(ob.segments_asks.is_empty());

        ob.on_add_order_no_mpid(
            AddOrderNoMPID::new(&build_add_order_no_mpid(ID, b'B', 100, PRICE, 1)).unwrap(),
        );

        assert_eq!(ob.segments_bids.len(), 15);
        assert!(
            ob.segments_bids[..14]
                .iter()
                .all(std::option::Option::is_none)
        );
        assert!(ob.segments_bids[14].is_some());
        assert!(ob.segments_asks.is_empty());

        let level = ob.segments_bids[14].as_ref().unwrap()[999_500];
        assert_eq!(level.count, 1);
        assert_eq!(ob.best_bid, u64::from(PRICE));
    }

    #[test]
    fn prices_across_segment_boundaries() {
        let mut ob = OrderBook::default();
        ob.on_add_order_no_mpid(
            AddOrderNoMPID::new(&build_add_order_no_mpid(1, b'B', 10, 999_999, 1)).unwrap(),
        );
        ob.on_add_order_no_mpid(
            AddOrderNoMPID::new(&build_add_order_no_mpid(2, b'B', 10, 1_000_000, 2)).unwrap(),
        );
        ob.on_add_order_no_mpid(
            AddOrderNoMPID::new(&build_add_order_no_mpid(3, b'B', 10, 1_500_000, 3)).unwrap(),
        );

        assert_eq!(
            ob.get_price_level(Side::Buy, Price::from(999_999))
                .unwrap()
                .count,
            1
        );
        assert_eq!(
            ob.get_price_level(Side::Buy, Price::from(1_000_000))
                .unwrap()
                .count,
            1
        );
        assert_eq!(
            ob.get_price_level(Side::Buy, Price::from(1_500_000))
                .unwrap()
                .count,
            1
        );
        assert_eq!(ob.best_bid, 1_500_000);

        // removing the best bid walks down across the segment boundary
        ob.on_order_delete(OrderDelete::new(&build_order_delete(3, 100)).unwrap());
        assert_eq!(ob.best_bid, 1_000_000);
    }

    #[test]
    #[should_panic(expected = "Price segment exceeds MAX_SEGMENTS")]
    fn add_order_beyond_max_segments_panics() {
        const ID: u64 = 1;
        const PRICE: u32 = 2_000_000_000; // segment 2000 = MAX_SEGMENTS

        let mut ob = OrderBook::default();
        let buf = build_add_order_no_mpid(ID, b'B', 100, PRICE, 1);
        ob.on_add_order_no_mpid(AddOrderNoMPID::new(&buf).unwrap());
    }
}
