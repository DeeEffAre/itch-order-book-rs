use std::{
    cmp::Reverse,
    collections::{BTreeMap, HashMap, VecDeque},
};

use crate::itch_parser::{
    add_order::{AddOrderNoMPID, AddOrderWithMPID},
    handler::MessageHandler,
    modify_order::{
        order_cancel::OrderCancel,
        order_delete::OrderDelete,
        order_execute::{OrderExecute, OrderExecuteWithPrice},
        order_replace::OrderReplace,
    },
};

mod order;

#[derive(Default)]
pub struct OrderBook {
    // bids sorted high to low
    pub bids: BTreeMap<Reverse<order::Price>, VecDeque<order::Id>>,

    // asks sorted low to high
    pub asks: BTreeMap<order::Price, VecDeque<order::Id>>,

    pub order_pool: HashMap<order::Id, order::Order>,
}

impl MessageHandler for OrderBook {
    fn on_add_order_no_mpid(&mut self, msg: &AddOrderNoMPID) {
        let order = order::Order::try_from(msg).expect("Invalid add order format");
        self.order_pool.insert(order.id, order.clone());

        match order.side {
            order::Side::Buy => {
                if self.bids.contains_key(&Reverse(order.price)) {
                    let vdeq = self.bids.get_mut(&Reverse(order.price)).unwrap();
                    vdeq.push_back(order.id);
                } else {
                    let order_price = order.price;
                    let mut vdeq = VecDeque::new();
                    vdeq.push_back(order.id);
                    self.bids.insert(Reverse(order_price), vdeq);
                }
            }
            order::Side::Sell => {
                if self.asks.contains_key(&order.price) {
                    let vdeq = self.asks.get_mut(&order.price).unwrap();
                    vdeq.push_back(order.id);
                } else {
                    let order_price = order.price;
                    let mut vdeq = VecDeque::new();
                    vdeq.push_back(order.id);
                    self.asks.insert(order_price, vdeq);
                }
            }
        }
    }

    fn on_add_order_with_mpid(&mut self, msg: &AddOrderWithMPID) {
        let order = order::Order::try_from(msg).expect("Invalid add order format");
        self.order_pool.insert(order.id, order.clone());

        match order.side {
            order::Side::Buy => {
                if self.bids.contains_key(&Reverse(order.price)) {
                    let vdeq = self.bids.get_mut(&Reverse(order.price)).unwrap();
                    vdeq.push_back(order.id);
                } else {
                    let order_price = order.price;
                    let mut vdeq = VecDeque::new();
                    vdeq.push_back(order.id);
                    self.bids.insert(Reverse(order_price), vdeq);
                }
            }
            order::Side::Sell => {
                if self.asks.contains_key(&order.price) {
                    let vdeq = self.asks.get_mut(&order.price).unwrap();
                    vdeq.push_back(order.id);
                } else {
                    let order_price = order.price;
                    let mut vdeq = VecDeque::new();
                    vdeq.push_back(order.id);
                    self.asks.insert(order_price, vdeq);
                }
            }
        }
    }

    fn on_order_execute_with_price(&mut self, msg: &OrderExecuteWithPrice) {
        let order_id = order::Id(msg.order_ref_number.get());

        let Some(order) = self.order_pool.get_mut(&order_id) else {
            return;
        };

        order.shares -= msg.executed_shares.get();
        let fully_done = order.shares.0 == 0;

        if !fully_done {
            return;
        }

        let Some(order) = self.order_pool.remove(&order_id) else {
            return;
        };

        self.remove_from_queue(order.side, order.price, order.id);
    }

    fn on_order_execute(&mut self, msg: &OrderExecute) {
        let order_id = order::Id(msg.order_ref_number.get());

        let Some(order) = self.order_pool.get_mut(&order_id) else {
            return;
        };

        order.shares -= msg.executed_shares.get();
        let fully_done = order.shares.0 == 0;

        if !fully_done {
            return;
        }

        let Some(order) = self.order_pool.remove(&order_id) else {
            return;
        };

        self.remove_from_queue(order.side, order.price, order.id);
    }

    fn on_order_delete(&mut self, msg: &OrderDelete) {
        let order_id = order::Id(msg.order_ref_number.get());

        let Some(order) = self.order_pool.remove(&order_id) else {
            return;
        };

        self.remove_from_queue(order.side, order.price, order.id);
    }

    fn on_order_cancel(&mut self, msg: &OrderCancel) {
        let order_id = order::Id(msg.order_ref_number.get());

        let Some(order) = self.order_pool.get_mut(&order_id) else {
            return;
        };

        order.shares -= msg.cancelled_shares.get();
        let fully_done = order.shares.0 == 0;

        if !fully_done {
            return;
        }

        let Some(order) = self.order_pool.remove(&order_id) else {
            return;
        };

        self.remove_from_queue(order.side, order.price, order.id);
    }

    fn on_order_replace(&mut self, msg: &OrderReplace) {
        let order_id = order::Id(msg.org_order_ref_number.get());

        let Some(order) = self.order_pool.remove(&order_id) else {
            return;
        };
        // order replace doesn't contain the side info
        let side = order.side;

        self.remove_from_queue(order.side, order.price, order.id);

        let mut order = order::Order::try_from(msg).expect("Invalid replace order format");
        order.side = side;

        self.order_pool.insert(order.id, order.clone());

        match order.side {
            order::Side::Buy => {
                if self.bids.contains_key(&Reverse(order.price)) {
                    let vdeq = self.bids.get_mut(&Reverse(order.price)).unwrap();
                    vdeq.push_back(order.id);
                } else {
                    let order_price = order.price;
                    let mut vdeq = VecDeque::new();
                    vdeq.push_back(order.id);
                    self.bids.insert(Reverse(order_price), vdeq);
                }
            }
            order::Side::Sell => {
                if self.asks.contains_key(&order.price) {
                    let vdeq = self.asks.get_mut(&order.price).unwrap();
                    vdeq.push_back(order.id);
                } else {
                    let order_price = order.price;
                    let mut vdeq = VecDeque::new();
                    vdeq.push_back(order.id);
                    self.asks.insert(order_price, vdeq);
                }
            }
        }
    }
}

impl OrderBook {
    fn remove_from_queue(&mut self, side: order::Side, price: order::Price, id: order::Id) {
        let removed = match side {
            order::Side::Buy => self.bids.get_mut(&Reverse(price)).map(|q| {
                q.retain(|&x| x != id);
                q.is_empty()
            }),
            order::Side::Sell => self.asks.get_mut(&price).map(|q| {
                q.retain(|&x| x != id);
                q.is_empty()
            }),
        };

        if matches!(removed, Some(true)) {
            match side {
                order::Side::Buy => {
                    self.bids.remove(&Reverse(price));
                }
                order::Side::Sell => {
                    self.asks.remove(&price);
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::itch_parser::Message;
    use crate::order_book::order::{Id, Price, Quantity, Side, Timestamp};

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

        let expected_price = Reverse(Price::from(PRICE));
        assert!(ob.bids.contains_key(&expected_price));
        let orders = ob.bids.get(&expected_price).unwrap();
        assert_eq!(orders.len(), 1);
        assert_eq!(orders[0], Id(ID));
        let order = ob.order_pool.get(&orders[0]).unwrap();
        assert_eq!(order.shares, Quantity(SHARES));
        assert_eq!(order.side, Side::Buy);
        assert_eq!(order.price, Price::from(PRICE));
        assert_eq!(order.timestamp, Timestamp(TS));
        assert!(ob.asks.is_empty());
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

        let expected_price = Price::from(PRICE);
        assert!(ob.asks.contains_key(&expected_price));
        let orders = ob.asks.get(&expected_price).unwrap();
        assert_eq!(orders.len(), 1);
        assert_eq!(orders[0], Id(ID));
        let order = ob.order_pool.get(&orders[0]).unwrap();
        assert_eq!(order.shares, Quantity(SHARES));
        assert_eq!(order.side, Side::Sell);
        assert_eq!(order.price, Price::from(PRICE));
        assert_eq!(order.timestamp, Timestamp(TS));
        assert!(ob.bids.is_empty());
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

        let expected_price = Reverse(Price::from(PRICE));
        let orders = ob.bids.get(&expected_price).unwrap();
        assert_eq!(orders.len(), 2);
        assert_eq!(orders[0], Id(ID_1));
        assert_eq!(orders[1], Id(ID_2));
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

        let expected_price = Price::from(PRICE);
        let orders = ob.asks.get(&expected_price).unwrap();
        assert_eq!(orders.len(), 2);
        assert_eq!(orders[0], Id(ID_1));
        assert_eq!(orders[1], Id(ID_2));
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

        let prices: Vec<u64> = ob.bids.keys().map(|r| r.0.0).collect();
        assert_eq!(
            prices,
            vec![
                Price::from(PRICE_HIGH).0,
                Price::from(PRICE_MID).0,
                Price::from(PRICE_LOW).0
            ]
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

        let prices: Vec<u64> = ob.asks.keys().map(|p| p.0).collect();
        assert_eq!(
            prices,
            vec![
                Price::from(PRICE_LOW).0,
                Price::from(PRICE_MID).0,
                Price::from(PRICE_HIGH).0
            ]
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

        let expected_price = Reverse(Price::from(PRICE));
        assert!(ob.bids.contains_key(&expected_price));
        let orders = ob.bids.get(&expected_price).unwrap();
        assert_eq!(orders.len(), 1);
        assert_eq!(orders[0], Id(ID));
        let order = ob.order_pool.get(&orders[0]).unwrap();
        assert_eq!(order.shares, Quantity(SHARES));
        assert_eq!(order.side, Side::Buy);
        assert_eq!(order.price, Price::from(PRICE));
        assert_eq!(order.timestamp, Timestamp(TS));
        assert!(ob.asks.is_empty());
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

        let expected_price = Price::from(PRICE);
        assert!(ob.asks.contains_key(&expected_price));
        let orders = ob.asks.get(&expected_price).unwrap();
        assert_eq!(orders.len(), 1);
        assert_eq!(orders[0], Id(ID));
        let order = ob.order_pool.get(&orders[0]).unwrap();
        assert_eq!(order.shares, Quantity(SHARES));
        assert_eq!(order.side, Side::Sell);
        assert_eq!(order.price, Price::from(PRICE));
        assert_eq!(order.timestamp, Timestamp(TS));
        assert!(ob.bids.is_empty());
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

        assert_eq!(ob.bids.len(), 1);
        assert_eq!(ob.asks.len(), 1);
        assert_eq!(
            ob.bids.get(&Reverse(Price::from(BUY_PRICE))).unwrap()[0],
            Id(BUY_ID)
        );
        assert_eq!(
            ob.asks.get(&Price::from(SELL_PRICE)).unwrap()[0],
            Id(SELL_ID)
        );
    }

    #[test]
    fn empty_order_book() {
        let ob = OrderBook::default();
        assert!(ob.bids.is_empty());
        assert!(ob.asks.is_empty());
        assert!(ob.order_pool.is_empty());
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

        let expected_price = Reverse(Price::from(PRICE));
        assert!(
            !ob.bids.contains_key(&expected_price),
            "price level should be removed after last order executed"
        );
        assert!(
            !ob.order_pool.contains_key(&Id(ID)),
            "order should be removed from pool"
        );
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

        let expected_price = Reverse(Price::from(PRICE));
        assert!(
            ob.bids.contains_key(&expected_price),
            "queue should still exist"
        );
        assert_eq!(ob.bids.get(&expected_price).unwrap().len(), 1);
        assert_eq!(ob.bids.get(&expected_price).unwrap()[0], Id(ID));
        let order = ob.order_pool.get(&Id(ID)).unwrap();
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

        let expected_price = Reverse(Price::from(PRICE));
        assert!(!ob.bids.contains_key(&expected_price));
        assert!(!ob.order_pool.contains_key(&Id(ID)));
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

        let expected_price = Price::from(PRICE);
        assert!(!ob.asks.contains_key(&expected_price));
        assert!(!ob.order_pool.contains_key(&Id(ID)));
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

        let expected_price = Reverse(Price::from(PRICE));
        assert!(!ob.bids.contains_key(&expected_price));
        assert!(!ob.order_pool.contains_key(&Id(ID)));
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

        let expected_price = Reverse(Price::from(PRICE));
        assert!(ob.bids.contains_key(&expected_price));
        assert_eq!(ob.bids.get(&expected_price).unwrap()[0], Id(ID));
        let order = ob.order_pool.get(&Id(ID)).unwrap();
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

        let expected_price = Reverse(Price::from(PRICE));
        assert!(!ob.bids.contains_key(&expected_price));
        assert!(!ob.order_pool.contains_key(&Id(ID)));
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

        let expected_price = Reverse(Price::from(PRICE));
        let orders = ob.bids.get(&expected_price).unwrap();
        assert_eq!(orders.len(), 1);
        assert_eq!(orders[0], Id(ID_2));
        assert!(!ob.order_pool.contains_key(&Id(ID_1)));
        assert!(ob.order_pool.contains_key(&Id(ID_2)));
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

        let expected_price = Price::from(PRICE);
        assert!(!ob.asks.contains_key(&expected_price));
        assert!(!ob.order_pool.contains_key(&Id(ID)));
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

        let expected_price = Reverse(Price::from(PRICE));
        assert!(!ob.bids.contains_key(&expected_price));
        assert!(!ob.order_pool.contains_key(&Id(ID)));
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

        let expected_price = Reverse(Price::from(PRICE));
        assert!(ob.bids.contains_key(&expected_price));
        assert_eq!(ob.bids.get(&expected_price).unwrap()[0], Id(ID));
        let order = ob.order_pool.get(&Id(ID)).unwrap();
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

        let expected_price = Reverse(Price::from(PRICE));
        assert!(!ob.bids.contains_key(&expected_price));
        assert!(!ob.order_pool.contains_key(&Id(ID)));
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

        let expected_price = Price::from(PRICE);
        assert!(!ob.asks.contains_key(&expected_price));
        assert!(!ob.order_pool.contains_key(&Id(ID)));
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
        assert!(!ob.bids.contains_key(&Reverse(Price::from(OLD_PRICE))));
        assert!(!ob.order_pool.contains_key(&Id(OLD_ID)));

        // new order at new price level
        let new_expected_price = Reverse(Price::from(NEW_PRICE));
        assert!(ob.bids.contains_key(&new_expected_price));
        let orders = ob.bids.get(&new_expected_price).unwrap();
        assert_eq!(orders.len(), 1);
        assert_eq!(orders[0], Id(NEW_ID));
        let new_order = ob.order_pool.get(&Id(NEW_ID)).unwrap();
        assert_eq!(new_order.shares, Quantity(NEW_SHARES));
        assert_eq!(new_order.side, Side::Buy);
        assert_eq!(new_order.price, Price::from(NEW_PRICE));
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

        let expected_price = Price::from(PRICE);
        assert!(ob.asks.contains_key(&expected_price));
        let orders = ob.asks.get(&expected_price).unwrap();
        assert_eq!(orders.len(), 2);
        assert_eq!(orders[0], Id(102)); // original second order still first
        assert_eq!(orders[1], Id(NEW_ID)); // replaced order goes to back of queue

        assert!(!ob.order_pool.contains_key(&Id(OLD_ID)));
        assert!(ob.order_pool.contains_key(&Id(NEW_ID)));
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

        assert!(ob.asks.contains_key(&Price::from(PRICE)));
        let new_order = ob.order_pool.get(&Id(NEW_ID)).unwrap();
        assert_eq!(new_order.side, Side::Sell);
    }
}
