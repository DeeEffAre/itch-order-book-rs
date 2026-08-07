use std::{
    cmp::Reverse,
    collections::{BTreeMap, HashMap, VecDeque},
};

use crate::itch_parser::{
    add_order::{AddOrderNoMPID, AddOrderWithMPID},
    handler::MessageHandler,
    modify_order::order_execute::OrderExecuteWithPrice,
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
        self.order_pool
            .insert(msg.order_ref_number.get().into(), order.clone());

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
        self.order_pool
            .insert(msg.order_ref_number.get().into(), order.clone());

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
        let order_id = msg.order_ref_number.get();

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
}

impl OrderBook {
    fn remove_from_queue(&mut self, side: order::Side, price: order::Price, id: order::Id) {
        let removed = match side {
            order::Side::Buy => self.bids.get_mut(&Reverse(price)).map(|q| {
                if let Some(idx) = q.iter().position(|&x| x == id) {
                    q.remove(idx);
                }
                q.is_empty()
            }),
            order::Side::Sell => self.asks.get_mut(&price).map(|q| {
                if let Some(idx) = q.iter().position(|&x| x == id) {
                    q.remove(idx);
                }
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
}
