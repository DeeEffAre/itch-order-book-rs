use std::ops::{Index, IndexMut};

use crate::{
    NumericConversions,
    itch_parser::{
        add_order::{AddOrderNoMPID, AddOrderWithMPID},
        modify_order::order_replace::OrderReplace,
        system_event_message::SystemEvent,
    },
    order_book::bitset::PriceBitset,
};

mod bitset;
mod order;

const TICK_SIZE: u64 = 100; // 1-cent tick (0.01$ = 100 in 4dp)
const MAX_PRICE: usize = 100_000; // 100K slots = $1000 range after scaling

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct SlabIndex(pub u32);

impl SlabIndex {
    pub const NULL: Self = Self(u32::MAX);

    #[inline]
    #[must_use]
    pub fn is_null(self) -> bool {
        self.0 == u32::MAX
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Volume(u32);

impl std::ops::AddAssign<u32> for Volume {
    #[inline]
    fn add_assign(&mut self, rhs: u32) {
        self.0 += rhs;
    }
}

impl std::ops::SubAssign<u32> for Volume {
    #[inline]
    fn sub_assign(&mut self, rhs: u32) {
        self.0 -= rhs;
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

#[derive(Default, Clone)]
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
            let idx = self.orders.len().as_u32();
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
    pub fn len(&self) -> usize {
        self.orders.len() - self.free_list.len()
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

#[derive(Clone)]
pub struct OrderBook {
    // tick based price offset
    pub price_offset: Option<u64>,
    pub bids: Vec<PriceLevel>,
    pub asks: Vec<PriceLevel>,
    pub bid_bitmap: Box<PriceBitset>,
    pub ask_bitmap: Box<PriceBitset>,
    pub order_pool: Slab,
    pub best_bid: u32,
    pub best_ask: u32,
}

impl Default for OrderBook {
    fn default() -> Self {
        Self {
            price_offset: None,
            bids: vec![PriceLevel::default(); MAX_PRICE],
            asks: vec![PriceLevel::default(); MAX_PRICE],
            bid_bitmap: Box::default(),
            ask_bitmap: Box::default(),
            order_pool: Slab::default(),
            best_bid: 0,
            best_ask: u32::MAX,
        }
    }
}

impl OrderBook {
    #[inline]
    #[must_use]
    pub fn get_price_level(&self, side: order::Side, price: order::Price) -> Option<&PriceLevel> {
        let offset = self.price_offset?;

        let tick_price = price.0 / TICK_SIZE;

        if tick_price < offset {
            return None;
        }

        let idx = (tick_price - offset).as_idx();
        if idx >= MAX_PRICE {
            return None;
        }
        Some(if side == order::Side::Buy {
            &self.bids[idx]
        } else {
            &self.asks[idx]
        })
    }

    #[inline(never)]
    fn shift_prices_down(&mut self, shift: u64) {
        let shift = shift.as_idx();
        debug_assert!(shift < MAX_PRICE, "caller rejects shifts >= MAX_PRICE");

        // best_bid == 0 is both the "no bids" sentinel and a valid index (lowest price);
        // capture occupancy of slot 0 before the shift zeroes it out
        let has_bid_at_zero = self.bids[0].count > 0;

        // shift bids to right
        self.bids.copy_within(0..MAX_PRICE - shift, shift);
        for i in 0..shift {
            self.bids[i] = PriceLevel::default();
        }

        // shift asks to right
        self.asks.copy_within(0..MAX_PRICE - shift, shift);
        for i in 0..shift {
            self.asks[i] = PriceLevel::default();
        }

        self.bid_bitmap.shift_right(shift);
        self.ask_bitmap.shift_right(shift);

        // update bests
        if self.best_bid != 0 || has_bid_at_zero {
            self.best_bid = self.best_bid.saturating_add(shift.as_u32());
        }
        if self.best_ask != u32::MAX {
            self.best_ask = self.best_ask.saturating_add(shift.as_u32());
        }
    }

    fn add_order(&mut self, order: order::Order) -> Option<SlabIndex> {
        let price = order.price.0;

        // Tick scaling: 4dp wire value → 1-cent ticks
        let tick_price = price / TICK_SIZE;

        // set the offset with first order
        if self.price_offset.is_none() {
            self.price_offset = Some(tick_price);
        }

        let offset = self
            .price_offset
            .expect("price_offset should be Some when order exists");

        if tick_price < offset {
            let shift = offset - tick_price;
            if shift >= MAX_PRICE as u64 {
                // price is far below the window: reject like the high side.
                // the book must never clear itself, the engine owns the order map
                return None;
            }
            self.shift_prices_down(shift);
            self.price_offset = Some(tick_price);
        }

        let offset = self
            .price_offset
            .expect("price_offset should be Some when order exists");

        let price_idx = (tick_price - offset).as_idx();

        if price_idx >= MAX_PRICE {
            // reject out of band orders.
            return None;
        }

        let slab_idx = self.order_pool.insert(order);

        let level = if order.side == order::Side::Buy {
            self.best_bid = std::cmp::max(self.best_bid, price_idx.as_u32());
            &mut self.bids[price_idx]
        } else {
            self.best_ask = std::cmp::min(self.best_ask, price_idx.as_u32());
            &mut self.asks[price_idx]
        };

        if level.tail == SlabIndex::NULL {
            level.head = slab_idx;
            level.tail = slab_idx;
        } else {
            let old_tail = level.tail;
            self.order_pool[old_tail].next = slab_idx;
            self.order_pool[slab_idx].prev = old_tail;
            level.tail = slab_idx;
        }

        if level.count == 0 {
            if order.side == order::Side::Buy {
                self.bid_bitmap.set(price_idx);
            } else {
                self.ask_bitmap.set(price_idx);
            }
        }

        level.volume += order.shares.0;
        level.count += 1;

        Some(slab_idx)
    }

    fn remove_order_by_slab(&mut self, slab_idx: SlabIndex) {
        let order_copy = self.order_pool[slab_idx];
        let tick_price = order_copy.price.0 / TICK_SIZE;

        let offset = self
            .price_offset
            .expect("price_offset should be Some when order exists");

        let price_idx = (tick_price - offset).as_idx();

        if price_idx >= MAX_PRICE {
            return;
        }

        let level = if order_copy.side == order::Side::Buy {
            &mut self.bids[price_idx]
        } else {
            &mut self.asks[price_idx]
        };
        level.volume -= order_copy.shares.0;

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
            if order_copy.side == order::Side::Buy {
                self.bid_bitmap.clear(price_idx);
                if price_idx.as_u32() == self.best_bid {
                    self.best_bid = self
                        .bid_bitmap
                        .find_last_set_from(price_idx)
                        .map_or(0, super::NumericConversions::as_u32);
                }
            } else {
                self.ask_bitmap.clear(price_idx);
                if price_idx.as_u32() == self.best_ask {
                    self.best_ask = self
                        .ask_bitmap
                        .find_first_set_from(price_idx)
                        .map_or(u32::MAX, super::NumericConversions::as_u32);
                }
            }
        }
    }

    fn handle_execution(&mut self, slab_idx: SlabIndex, executed_shares: u32) -> bool {
        let mut order_copy = self.order_pool[slab_idx];
        order_copy.shares -= executed_shares;
        let fully_done = order_copy.shares.0 == 0;

        let tick_price = order_copy.price.0 / TICK_SIZE;
        let offset = self
            .price_offset
            .expect("price_offset should be Some when order exists");
        let price_idx = (tick_price - offset).as_idx();

        let level = if order_copy.side == order::Side::Buy {
            &mut self.bids[price_idx]
        } else {
            &mut self.asks[price_idx]
        };
        level.volume -= executed_shares;

        if fully_done {
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
                if order_copy.side == order::Side::Buy {
                    self.bid_bitmap.clear(price_idx);
                    if price_idx.as_u32() == self.best_bid {
                        self.best_bid = self
                            .bid_bitmap
                            .find_last_set_from(price_idx)
                            .map_or(0, super::NumericConversions::as_u32);
                    }
                } else {
                    self.ask_bitmap.clear(price_idx);
                    if price_idx.as_u32() == self.best_ask {
                        self.best_ask = self
                            .ask_bitmap
                            .find_first_set_from(price_idx)
                            .map_or(u32::MAX, super::NumericConversions::as_u32);
                    }
                }
            }
            true
        } else {
            self.order_pool[slab_idx] = order_copy;
            false
        }
    }

    #[inline]
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.order_pool.is_empty()
    }

    #[inline]
    fn clear_all(&mut self) {
        self.asks.clear();
        self.bids.clear();
        self.price_offset = None;
        self.order_pool.clear();
        self.best_bid = 0;
        self.best_ask = u32::MAX;
        self.bid_bitmap.l1.fill(0);
        self.bid_bitmap.l2.fill(0);
        self.ask_bitmap.l1.fill(0);
        self.ask_bitmap.l2.fill(0);
    }
}

impl OrderBook {
    #[inline]
    pub fn add_order_no_mpid(&mut self, msg: &AddOrderNoMPID) -> Option<SlabIndex> {
        let order = order::Order::try_from(msg).expect("Invalid add order format");
        self.add_order(order)
    }

    #[inline]
    pub fn add_order_with_mpid(&mut self, msg: &AddOrderWithMPID) -> Option<SlabIndex> {
        let order = order::Order::try_from(msg).expect("Invalid add order format");
        self.add_order(order)
    }

    /// Removes the order stored at `slab_idx` from its price level and frees the slot.
    pub fn remove_slab(&mut self, slab_idx: SlabIndex) {
        self.remove_order_by_slab(slab_idx);
    }

    /// Executes/cancels `executed_shares` on the order at `slab_idx`.
    /// Returns `true` if the order was fully consumed and removed from the book.
    pub fn handle_execution_slab(&mut self, slab_idx: SlabIndex, executed_shares: u32) -> bool {
        self.handle_execution(slab_idx, executed_shares)
    }

    /// Replaces the order at `old_slab_idx` with the new order carried by `msg`.
    /// Returns the slab index of the replacement, or `None` if it was out of band.
    pub fn replace_slab(
        &mut self,
        old_slab_idx: SlabIndex,
        msg: &OrderReplace,
    ) -> Option<SlabIndex> {
        let old_order = *self.order_pool.get(old_slab_idx);
        self.remove_order_by_slab(old_slab_idx);

        let mut new_order = order::Order::try_from(msg).expect("Invalid replace order format");
        new_order.side = old_order.side;

        self.add_order(new_order)
    }

    pub fn on_system_event(&mut self, msg: &SystemEvent) {
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

    fn best_bid_price(book: &OrderBook) -> u64 {
        (u64::from(book.best_bid) + book.price_offset.unwrap_or(0)) * TICK_SIZE
    }

    fn best_ask_price(book: &OrderBook) -> u64 {
        (u64::from(book.best_ask) + book.price_offset.unwrap_or(0)) * TICK_SIZE
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

        let slab = ob.add_order_no_mpid(msg).unwrap();

        let level = ob.get_price_level(Side::Buy, Price::from(PRICE)).unwrap();
        assert_eq!(level.count, 1);
        assert_eq!(level.volume.0, SHARES);
        assert_eq!(level_ids(&ob, Side::Buy, PRICE), vec![Id(ID)]);
        let order = ob.order_pool.get(slab);
        assert_eq!(order.shares, Quantity(SHARES));
        assert_eq!(order.side, Side::Buy);
        assert_eq!(order.price, Price::from(PRICE));
        assert_eq!(ob.best_ask, u32::MAX, "no asks should be present");
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

        let slab = ob.add_order_no_mpid(msg).unwrap();

        let level = ob.get_price_level(Side::Sell, Price::from(PRICE)).unwrap();
        assert_eq!(level.count, 1);
        assert_eq!(level.volume.0, SHARES);
        assert_eq!(level_ids(&ob, Side::Sell, PRICE), vec![Id(ID)]);
        let order = ob.order_pool.get(slab);
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

        ob.add_order_no_mpid(AddOrderNoMPID::new(&buf1).unwrap());
        ob.add_order_no_mpid(AddOrderNoMPID::new(&buf2).unwrap());

        let level = ob.get_price_level(Side::Buy, Price::from(PRICE)).unwrap();
        assert_eq!(level.count, 2);
        assert_eq!(level.volume.0, SHARES_1 + SHARES_2);
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

        ob.add_order_no_mpid(AddOrderNoMPID::new(&buf1).unwrap());
        ob.add_order_no_mpid(AddOrderNoMPID::new(&buf2).unwrap());

        let level = ob.get_price_level(Side::Sell, Price::from(PRICE)).unwrap();
        assert_eq!(level.count, 2);
        assert_eq!(level.volume.0, SHARES_1 + SHARES_2);
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

        ob.add_order_no_mpid(AddOrderNoMPID::new(&buf1).unwrap());
        ob.add_order_no_mpid(AddOrderNoMPID::new(&buf2).unwrap());
        ob.add_order_no_mpid(AddOrderNoMPID::new(&buf3).unwrap());

        assert_eq!(best_bid_price(&ob), u64::from(PRICE_HIGH));
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

        ob.add_order_no_mpid(AddOrderNoMPID::new(&buf1).unwrap());
        ob.add_order_no_mpid(AddOrderNoMPID::new(&buf2).unwrap());
        ob.add_order_no_mpid(AddOrderNoMPID::new(&buf3).unwrap());

        assert_eq!(best_ask_price(&ob), u64::from(PRICE_LOW));
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

        let slab = ob.add_order_with_mpid(msg).unwrap();

        let level = ob.get_price_level(Side::Buy, Price::from(PRICE)).unwrap();
        assert_eq!(level.count, 1);
        assert_eq!(level.volume.0, SHARES);
        assert_eq!(level_ids(&ob, Side::Buy, PRICE), vec![Id(ID)]);
        let order = ob.order_pool.get(slab);
        assert_eq!(order.shares, Quantity(SHARES));
        assert_eq!(order.side, Side::Buy);
        assert_eq!(order.price, Price::from(PRICE));
        assert_eq!(ob.best_ask, u32::MAX, "no asks should be present");
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

        let slab = ob.add_order_with_mpid(msg).unwrap();

        let level = ob.get_price_level(Side::Sell, Price::from(PRICE)).unwrap();
        assert_eq!(level.count, 1);
        assert_eq!(level.volume.0, SHARES);
        assert_eq!(level_ids(&ob, Side::Sell, PRICE), vec![Id(ID)]);
        let order = ob.order_pool.get(slab);
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

        ob.add_order_with_mpid(AddOrderWithMPID::new(&buf_buy).unwrap());
        ob.add_order_with_mpid(AddOrderWithMPID::new(&buf_sell).unwrap());

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
        assert_eq!(best_bid_price(&ob), u64::from(BUY_PRICE));
        assert_eq!(best_ask_price(&ob), u64::from(SELL_PRICE));
    }

    #[test]
    fn empty_order_book() {
        let ob = OrderBook::default();
        assert!(ob.is_empty());
        assert_eq!(ob.best_bid, 0);
        assert_eq!(ob.best_ask, u32::MAX);
        assert!(ob.price_offset.is_none());
        assert_eq!(ob.bids.len(), MAX_PRICE);
        assert_eq!(ob.asks.len(), MAX_PRICE);
    }

    // ---- on_order_execute ----

    #[test]
    fn execute_full_buy_order() {
        const ID: u64 = 10;
        const PRICE: u32 = 10000;
        const SHARES: u32 = 200;

        let mut ob = OrderBook::default();
        let slab = ob
            .add_order_no_mpid(
                AddOrderNoMPID::new(&build_add_order_no_mpid(ID, b'B', SHARES, PRICE, 1)).unwrap(),
            )
            .unwrap();

        assert!(ob.handle_execution_slab(slab, SHARES));

        let level = ob.get_price_level(Side::Buy, Price::from(PRICE)).unwrap();
        assert_eq!(
            level.count, 0,
            "price level should be empty after last order executed"
        );
        assert_eq!(ob.order_pool.len(), 0, "order should be removed from pool");
        assert_eq!(ob.best_bid, 0);
    }

    #[test]
    fn execute_partial_buy_order() {
        const ID: u64 = 11;
        const PRICE: u32 = 10000;
        const INITIAL_SHARES: u32 = 200;
        const EXECUTED: u32 = 80;

        let mut ob = OrderBook::default();
        let slab = ob
            .add_order_no_mpid(
                AddOrderNoMPID::new(&build_add_order_no_mpid(ID, b'B', INITIAL_SHARES, PRICE, 1))
                    .unwrap(),
            )
            .unwrap();

        assert!(!ob.handle_execution_slab(slab, EXECUTED));

        let level = ob.get_price_level(Side::Buy, Price::from(PRICE)).unwrap();
        assert_eq!(level.count, 1, "queue should still exist");
        assert_eq!(level.volume.0, INITIAL_SHARES - EXECUTED);
        assert_eq!(level_ids(&ob, Side::Buy, PRICE), vec![Id(ID)]);
        let order = ob.order_pool.get(slab);
        assert_eq!(order.shares, Quantity(INITIAL_SHARES - EXECUTED));
    }

    #[test]
    fn execute_two_partials_become_full() {
        const ID: u64 = 12;
        const PRICE: u32 = 10000;
        const INITIAL_SHARES: u32 = 200;

        let mut ob = OrderBook::default();
        let slab = ob
            .add_order_no_mpid(
                AddOrderNoMPID::new(&build_add_order_no_mpid(ID, b'B', INITIAL_SHARES, PRICE, 1))
                    .unwrap(),
            )
            .unwrap();

        assert!(!ob.handle_execution_slab(slab, 50));
        assert!(ob.handle_execution_slab(slab, INITIAL_SHARES - 50));

        assert_eq!(
            ob.get_price_level(Side::Buy, Price::from(PRICE))
                .unwrap()
                .count,
            0
        );
        assert_eq!(ob.order_pool.len(), 0);
        assert_eq!(ob.best_bid, 0);
    }

    #[test]
    fn execute_full_sell_order() {
        const ID: u64 = 20;
        const PRICE: u32 = 15000;
        const SHARES: u32 = 300;

        let mut ob = OrderBook::default();
        let slab = ob
            .add_order_no_mpid(
                AddOrderNoMPID::new(&build_add_order_no_mpid(ID, b'S', SHARES, PRICE, 1)).unwrap(),
            )
            .unwrap();

        assert!(ob.handle_execution_slab(slab, SHARES));

        assert_eq!(
            ob.get_price_level(Side::Sell, Price::from(PRICE))
                .unwrap()
                .count,
            0
        );
        assert_eq!(ob.order_pool.len(), 0);
        assert_eq!(ob.best_ask, u32::MAX);
    }

    // ---- on_order_execute_with_price ----

    #[test]
    fn execute_with_price_full_buy_order() {
        const ID: u64 = 30;
        const PRICE: u32 = 10000;
        const SHARES: u32 = 100;

        let mut ob = OrderBook::default();
        let slab = ob
            .add_order_no_mpid(
                AddOrderNoMPID::new(&build_add_order_no_mpid(ID, b'B', SHARES, PRICE, 1)).unwrap(),
            )
            .unwrap();

        assert!(ob.handle_execution_slab(slab, SHARES));

        assert_eq!(
            ob.get_price_level(Side::Buy, Price::from(PRICE))
                .unwrap()
                .count,
            0
        );
        assert_eq!(ob.order_pool.len(), 0);
        assert_eq!(ob.best_bid, 0);
    }

    #[test]
    fn execute_with_price_partial_buy_order() {
        const ID: u64 = 31;
        const PRICE: u32 = 10000;
        const INITIAL_SHARES: u32 = 200;
        const EXECUTED: u32 = 75;

        let mut ob = OrderBook::default();
        let slab = ob
            .add_order_no_mpid(
                AddOrderNoMPID::new(&build_add_order_no_mpid(ID, b'B', INITIAL_SHARES, PRICE, 1))
                    .unwrap(),
            )
            .unwrap();

        assert!(!ob.handle_execution_slab(slab, EXECUTED));

        let level = ob.get_price_level(Side::Buy, Price::from(PRICE)).unwrap();
        assert_eq!(level.count, 1);
        assert_eq!(level.volume.0, INITIAL_SHARES - EXECUTED);
        assert_eq!(level_ids(&ob, Side::Buy, PRICE), vec![Id(ID)]);
        let order = ob.order_pool.get(slab);
        assert_eq!(order.shares, Quantity(INITIAL_SHARES - EXECUTED));
    }

    // ---- on_order_delete ----

    #[test]
    fn delete_existing_buy_order() {
        const ID: u64 = 40;
        const PRICE: u32 = 10000;
        const SHARES: u32 = 100;

        let mut ob = OrderBook::default();
        let slab = ob
            .add_order_no_mpid(
                AddOrderNoMPID::new(&build_add_order_no_mpid(ID, b'B', SHARES, PRICE, 1)).unwrap(),
            )
            .unwrap();

        ob.remove_slab(slab);

        assert_eq!(
            ob.get_price_level(Side::Buy, Price::from(PRICE))
                .unwrap()
                .count,
            0
        );
        assert_eq!(ob.order_pool.len(), 0);
        assert_eq!(ob.best_bid, 0);
    }

    #[test]
    fn delete_one_of_two_at_same_price() {
        const ID_1: u64 = 50;
        const ID_2: u64 = 51;
        const PRICE: u32 = 10000;
        const SHARES: u32 = 100;

        let mut ob = OrderBook::default();
        let slab_1 = ob
            .add_order_no_mpid(
                AddOrderNoMPID::new(&build_add_order_no_mpid(ID_1, b'B', SHARES, PRICE, 1))
                    .unwrap(),
            )
            .unwrap();
        let _slab_2 = ob
            .add_order_no_mpid(
                AddOrderNoMPID::new(&build_add_order_no_mpid(ID_2, b'B', SHARES, PRICE, 2))
                    .unwrap(),
            )
            .unwrap();

        ob.remove_slab(slab_1);

        let level = ob.get_price_level(Side::Buy, Price::from(PRICE)).unwrap();
        assert_eq!(level.count, 1);
        assert_eq!(level_ids(&ob, Side::Buy, PRICE), vec![Id(ID_2)]);
        assert_eq!(ob.order_pool.len(), 1);
        assert_eq!(best_bid_price(&ob), u64::from(PRICE));
    }

    #[test]
    fn delete_existing_sell_order() {
        const ID: u64 = 60;
        const PRICE: u32 = 15000;
        const SHARES: u32 = 200;

        let mut ob = OrderBook::default();
        let slab = ob
            .add_order_no_mpid(
                AddOrderNoMPID::new(&build_add_order_no_mpid(ID, b'S', SHARES, PRICE, 1)).unwrap(),
            )
            .unwrap();

        ob.remove_slab(slab);

        assert_eq!(
            ob.get_price_level(Side::Sell, Price::from(PRICE))
                .unwrap()
                .count,
            0
        );
        assert_eq!(ob.order_pool.len(), 0);
        assert_eq!(ob.best_ask, u32::MAX);
    }

    // ---- on_order_cancel ----

    #[test]
    fn cancel_all_shares_removes_order() {
        const ID: u64 = 70;
        const PRICE: u32 = 10000;
        const SHARES: u32 = 500;

        let mut ob = OrderBook::default();
        let slab = ob
            .add_order_no_mpid(
                AddOrderNoMPID::new(&build_add_order_no_mpid(ID, b'B', SHARES, PRICE, 1)).unwrap(),
            )
            .unwrap();

        assert!(ob.handle_execution_slab(slab, SHARES));

        assert_eq!(
            ob.get_price_level(Side::Buy, Price::from(PRICE))
                .unwrap()
                .count,
            0
        );
        assert_eq!(ob.order_pool.len(), 0);
        assert_eq!(ob.best_bid, 0);
    }

    #[test]
    fn cancel_partial_shares_keeps_order() {
        const ID: u64 = 71;
        const PRICE: u32 = 10000;
        const INITIAL_SHARES: u32 = 500;
        const CANCELLED: u32 = 200;

        let mut ob = OrderBook::default();
        let slab = ob
            .add_order_no_mpid(
                AddOrderNoMPID::new(&build_add_order_no_mpid(ID, b'B', INITIAL_SHARES, PRICE, 1))
                    .unwrap(),
            )
            .unwrap();

        assert!(!ob.handle_execution_slab(slab, CANCELLED));

        let level = ob.get_price_level(Side::Buy, Price::from(PRICE)).unwrap();
        assert_eq!(level.count, 1);
        assert_eq!(level.volume.0, INITIAL_SHARES - CANCELLED);
        assert_eq!(level_ids(&ob, Side::Buy, PRICE), vec![Id(ID)]);
        let order = ob.order_pool.get(slab);
        assert_eq!(order.shares, Quantity(INITIAL_SHARES - CANCELLED));
    }

    #[test]
    fn cancel_two_partials_become_full() {
        const ID: u64 = 72;
        const PRICE: u32 = 10000;
        const INITIAL_SHARES: u32 = 400;

        let mut ob = OrderBook::default();
        let slab = ob
            .add_order_no_mpid(
                AddOrderNoMPID::new(&build_add_order_no_mpid(ID, b'B', INITIAL_SHARES, PRICE, 1))
                    .unwrap(),
            )
            .unwrap();

        assert!(!ob.handle_execution_slab(slab, 150));
        assert!(ob.handle_execution_slab(slab, INITIAL_SHARES - 150));

        assert_eq!(
            ob.get_price_level(Side::Buy, Price::from(PRICE))
                .unwrap()
                .count,
            0
        );
        assert_eq!(ob.order_pool.len(), 0);
        assert_eq!(ob.best_bid, 0);
    }

    #[test]
    fn cancel_all_sell_order() {
        const ID: u64 = 80;
        const PRICE: u32 = 15000;
        const SHARES: u32 = 300;

        let mut ob = OrderBook::default();
        let slab = ob
            .add_order_no_mpid(
                AddOrderNoMPID::new(&build_add_order_no_mpid(ID, b'S', SHARES, PRICE, 1)).unwrap(),
            )
            .unwrap();

        assert!(ob.handle_execution_slab(slab, SHARES));

        assert_eq!(
            ob.get_price_level(Side::Sell, Price::from(PRICE))
                .unwrap()
                .count,
            0
        );
        assert_eq!(ob.order_pool.len(), 0);
        assert_eq!(ob.best_ask, u32::MAX);
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
        let old_slab = ob
            .add_order_no_mpid(
                AddOrderNoMPID::new(&build_add_order_no_mpid(
                    OLD_ID, b'B', OLD_SHARES, OLD_PRICE, 1,
                ))
                .unwrap(),
            )
            .unwrap();

        let buf = build_order_replace(OLD_ID, NEW_ID, NEW_SHARES, NEW_PRICE, 200);
        let new_slab = ob
            .replace_slab(old_slab, OrderReplace::new(&buf).unwrap())
            .unwrap();

        // old price level gone
        assert_eq!(
            ob.get_price_level(Side::Buy, Price::from(OLD_PRICE))
                .unwrap()
                .count,
            0
        );
        assert_eq!(ob.order_pool.len(), 1);

        // new order at new price level
        let level = ob
            .get_price_level(Side::Buy, Price::from(NEW_PRICE))
            .unwrap();
        assert_eq!(level.count, 1);
        assert_eq!(level_ids(&ob, Side::Buy, NEW_PRICE), vec![Id(NEW_ID)]);
        let new_order = ob.order_pool.get(new_slab);
        assert_eq!(new_order.shares, Quantity(NEW_SHARES));
        assert_eq!(new_order.side, Side::Buy);
        assert_eq!(new_order.price, Price::from(NEW_PRICE));
        assert_eq!(best_bid_price(&ob), u64::from(NEW_PRICE));
    }

    #[test]
    fn replace_sell_order_same_price() {
        const OLD_ID: u64 = 100;
        const NEW_ID: u64 = 101;
        const PRICE: u32 = 15000;
        const OLD_SHARES: u32 = 100;
        const NEW_SHARES: u32 = 300;

        let mut ob = OrderBook::default();
        let old_slab = ob
            .add_order_no_mpid(
                AddOrderNoMPID::new(&build_add_order_no_mpid(OLD_ID, b'S', OLD_SHARES, PRICE, 1))
                    .unwrap(),
            )
            .unwrap();

        // add another sell at same price so queue isn't empty after remove
        ob.add_order_no_mpid(
            AddOrderNoMPID::new(&build_add_order_no_mpid(102, b'S', 50, PRICE, 2)).unwrap(),
        );

        let buf = build_order_replace(OLD_ID, NEW_ID, NEW_SHARES, PRICE, 200);
        let _new_slab = ob
            .replace_slab(old_slab, OrderReplace::new(&buf).unwrap())
            .unwrap();

        let level = ob.get_price_level(Side::Sell, Price::from(PRICE)).unwrap();
        assert_eq!(level.count, 2);
        assert_eq!(level_ids(&ob, Side::Sell, PRICE), vec![Id(102), Id(NEW_ID)]);

        assert_eq!(ob.order_pool.len(), 2);
    }

    #[test]
    fn replace_preserves_side() {
        const OLD_ID: u64 = 110;
        const NEW_ID: u64 = 111;
        const PRICE: u32 = 10000;

        let mut ob = OrderBook::default();
        let old_slab = ob
            .add_order_no_mpid(
                AddOrderNoMPID::new(&build_add_order_no_mpid(OLD_ID, b'S', 100, PRICE, 1)).unwrap(),
            )
            .unwrap();

        let buf = build_order_replace(OLD_ID, NEW_ID, 200, PRICE, 200);
        let new_slab = ob
            .replace_slab(old_slab, OrderReplace::new(&buf).unwrap())
            .unwrap();

        assert_eq!(
            ob.get_price_level(Side::Sell, Price::from(PRICE))
                .unwrap()
                .count,
            1
        );
        let new_order = ob.order_pool.get(new_slab);
        assert_eq!(new_order.side, Side::Sell);
    }

    // ---- on_system_event ----

    #[test]
    fn system_event_clears_non_empty_book() {
        let mut ob = OrderBook::default();
        ob.add_order_no_mpid(
            AddOrderNoMPID::new(&build_add_order_no_mpid(1, b'B', 100, 10000, 1)).unwrap(),
        );
        ob.add_order_no_mpid(
            AddOrderNoMPID::new(&build_add_order_no_mpid(2, b'S', 50, 12000, 2)).unwrap(),
        );
        assert!(!ob.is_empty());

        let buf = build_system_event(b'O', 100);
        ob.on_system_event(SystemEvent::new(&buf).unwrap());

        assert!(ob.is_empty());
        assert_eq!(ob.best_bid, 0);
        assert_eq!(ob.best_ask, u32::MAX);
    }

    #[test]
    fn system_event_start_of_system_hours_clears() {
        let mut ob = OrderBook::default();
        ob.add_order_no_mpid(
            AddOrderNoMPID::new(&build_add_order_no_mpid(1, b'B', 100, 10000, 1)).unwrap(),
        );

        let buf = build_system_event(b'S', 100);
        ob.on_system_event(SystemEvent::new(&buf).unwrap());

        assert!(ob.is_empty());
    }

    #[test]
    fn system_event_end_of_system_hours_clears() {
        let mut ob = OrderBook::default();
        ob.add_order_no_mpid(
            AddOrderNoMPID::new(&build_add_order_no_mpid(1, b'B', 100, 10000, 1)).unwrap(),
        );

        let buf = build_system_event(b'E', 100);
        ob.on_system_event(SystemEvent::new(&buf).unwrap());

        assert!(ob.is_empty());
    }

    #[test]
    fn system_event_end_of_messages_clears() {
        let mut ob = OrderBook::default();
        ob.add_order_no_mpid(
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
        ob.add_order_no_mpid(
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
        ob.add_order_no_mpid(
            AddOrderNoMPID::new(&build_add_order_no_mpid(1, b'B', 100, 10000, 1)).unwrap(),
        );

        let buf = build_system_event(b'M', 100);
        ob.on_system_event(SystemEvent::new(&buf).unwrap());

        assert!(!ob.is_empty());
    }

    // ---- price offset sliding ----

    #[test]
    fn lower_price_shifts_offset_down() {
        const HIGH_ID: u64 = 1;
        const LOW_ID: u64 = 2;
        const HIGH_PRICE: u32 = 20_000;
        const LOW_PRICE: u32 = 10_000;

        let mut ob = OrderBook::default();
        let high_slab = ob
            .add_order_no_mpid(
                AddOrderNoMPID::new(&build_add_order_no_mpid(HIGH_ID, b'B', 100, HIGH_PRICE, 1))
                    .unwrap(),
            )
            .unwrap();
        assert_eq!(ob.price_offset, Some(u64::from(HIGH_PRICE) / TICK_SIZE));
        assert_eq!(ob.best_bid, 0);

        // lower price arrives -> offset slides down, existing level shifts right
        ob.add_order_no_mpid(
            AddOrderNoMPID::new(&build_add_order_no_mpid(LOW_ID, b'B', 100, LOW_PRICE, 2)).unwrap(),
        );
        assert_eq!(ob.price_offset, Some(u64::from(LOW_PRICE) / TICK_SIZE));
        assert_eq!(best_bid_price(&ob), u64::from(HIGH_PRICE));

        // both levels intact after the shift
        assert_eq!(
            ob.get_price_level(Side::Buy, Price::from(HIGH_PRICE))
                .unwrap()
                .count,
            1
        );
        assert_eq!(
            ob.get_price_level(Side::Buy, Price::from(LOW_PRICE))
                .unwrap()
                .count,
            1
        );
        assert_eq!(level_ids(&ob, Side::Buy, HIGH_PRICE), vec![Id(HIGH_ID)]);
        assert_eq!(level_ids(&ob, Side::Buy, LOW_PRICE), vec![Id(LOW_ID)]);

        // removal still works after the shift
        ob.remove_slab(high_slab);
        assert_eq!(best_bid_price(&ob), u64::from(LOW_PRICE));
        assert_eq!(ob.order_pool.len(), 1);
    }

    #[test]
    fn out_of_band_order_is_rejected() {
        const IN_BAND_ID: u64 = 1;
        const OUT_OF_BAND_ID: u64 = 2;
        const BASE_PRICE: u32 = 10_000; // tick 100
        const OUT_OF_BAND_PRICE: u32 = 10_010_000; // tick 100_100 -> idx 100_000 == MAX_PRICE

        let mut ob = OrderBook::default();
        ob.add_order_no_mpid(
            AddOrderNoMPID::new(&build_add_order_no_mpid(
                IN_BAND_ID, b'B', 100, BASE_PRICE, 1,
            ))
            .unwrap(),
        );

        // out-of-band add is silently dropped, no panic
        let buf = build_add_order_no_mpid(OUT_OF_BAND_ID, b'B', 100, OUT_OF_BAND_PRICE, 2);
        let rejected = ob.add_order_no_mpid(AddOrderNoMPID::new(&buf).unwrap());

        assert!(rejected.is_none());
        assert_eq!(ob.order_pool.len(), 1);
        assert!(
            ob.get_price_level(Side::Buy, Price::from(OUT_OF_BAND_PRICE))
                .is_none()
        );
        assert_eq!(level_ids(&ob, Side::Buy, BASE_PRICE), vec![Id(IN_BAND_ID)]);
        assert_eq!(ob.best_bid, 0);
    }
}
