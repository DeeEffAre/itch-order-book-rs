use rand::{RngExt, SeedableRng, rngs::StdRng};
use std::collections::HashMap;

pub const STOCK_LOCATE: u16 = 100;
pub const STOCK: [u8; 8] = *b"AAPL    ";
pub const MID_PRICE: u32 = 500_000; // $50.00, must stay below order_book::MAX_PRICE
pub const HALF_BAND: u32 = 30_000; // $3.00

#[allow(dead_code)]
pub fn message_len(message_type: u8) -> Option<usize> {
    match message_type {
        b'S' => Some(12),
        b'A' | b'C' => Some(36),
        b'F' => Some(40),
        b'X' => Some(23),
        b'D' => Some(19),
        b'U' => Some(35),
        b'E' => Some(31),
        _ => None,
    }
}

#[derive(Debug, Clone, Copy)]
pub struct Weights {
    pub add_no_mpid: u32,
    pub add_with_mpid: u32,
    pub cancel: u32,
    pub delete: u32,
    pub replace: u32,
    pub execute: u32,
    pub execute_with_price: u32,
}

impl Default for Weights {
    fn default() -> Self {
        Self {
            add_no_mpid: 75,
            add_with_mpid: 5,
            cancel: 10,
            delete: 2,
            replace: 3,
            execute: 4,
            execute_with_price: 1,
        }
    }
}

#[derive(Debug, Clone)]
pub struct SyntheticConfig {
    pub seed: u64,
    pub message_count: usize,
    pub prefill_orders: usize,
    pub weights: Weights,
    pub stock_locate: u16,
    pub stock: [u8; 8],
    pub mid_price: u32,
    pub half_band: u32,
}

impl Default for SyntheticConfig {
    fn default() -> Self {
        Self {
            seed: 42,
            message_count: 1_000_000,
            prefill_orders: 100_000,
            weights: Weights::default(),
            stock_locate: STOCK_LOCATE,
            stock: STOCK,
            mid_price: MID_PRICE,
            half_band: HALF_BAND,
        }
    }
}

pub struct Feed {
    pub prefill: Vec<Vec<u8>>,
    pub messages: Vec<Vec<u8>>,
    #[allow(dead_code)]
    pub final_active_orders: usize,
}

#[derive(Clone, Copy)]
struct ActiveOrder {
    side: u8,
    price: u32,
    shares: u32,
}

struct Generator {
    rng: StdRng,
    cfg: SyntheticConfig,
    orders: Vec<ActiveOrder>,
    active_ids: Vec<u64>,
    id_to_index: HashMap<u64, usize>,
    next_ref: u64,
    next_timestamp: u64,
    next_match: u64,
    tracking: u16,
}

impl Generator {
    fn new(cfg: SyntheticConfig) -> Self {
        Self {
            rng: StdRng::seed_from_u64(cfg.seed),
            cfg,
            orders: Vec::new(),
            active_ids: Vec::new(),
            id_to_index: HashMap::new(),
            next_ref: 1,
            next_timestamp: 9_000_000_000_000,
            next_match: 1,
            tracking: 1,
        }
    }

    fn insert(&mut self, id: u64, order: ActiveOrder) {
        self.id_to_index.insert(id, self.orders.len());
        self.orders.push(order);
        self.active_ids.push(id);
    }

    fn remove(&mut self, id: u64) {
        let Some(idx) = self.id_to_index.remove(&id) else {
            return;
        };
        let last = self.orders.len() - 1;
        self.orders.swap_remove(idx);
        self.active_ids.swap_remove(idx);
        if idx != last {
            let moved_id = self.active_ids[idx];
            self.id_to_index.insert(moved_id, idx);
        }
    }

    fn next_ts(&mut self) -> u64 {
        self.next_timestamp += self.rng.random_range(1..=10_000);
        self.next_timestamp
    }

    fn add_price(&mut self, side: u8) -> u32 {
        let offset = self.rng.random_range(0..=self.cfg.half_band);
        match side {
            b'B' => self.cfg.mid_price.saturating_sub(offset),
            _ => self.cfg.mid_price.saturating_add(offset),
        }
    }

    fn add(&mut self, with_mpid: bool) -> Vec<u8> {
        let id = self.next_ref;
        self.next_ref += 1;

        let side = if self.rng.random_bool(0.5) {
            b'B'
        } else {
            b'S'
        };
        let order = ActiveOrder {
            side,
            price: self.add_price(side),
            shares: self.rng.random_range(1..=1000),
        };
        self.insert(id, order);

        let len = if with_mpid { 40 } else { 36 };
        let mut buf = vec![0u8; len];
        buf[0] = if with_mpid { b'F' } else { b'A' };
        w16(&mut buf, 1, self.cfg.stock_locate);
        w16(&mut buf, 3, self.tracking);
        self.tracking = self.tracking.wrapping_add(1);
        w48(&mut buf, 5, self.next_ts());
        w64(&mut buf, 11, id);
        buf[19] = side;
        w32(&mut buf, 20, order.shares);
        w64(&mut buf, 24, u64::from_be_bytes(self.cfg.stock));
        w32(&mut buf, 32, order.price);
        if with_mpid {
            w32(&mut buf, 36, self.rng.random());
        }
        buf
    }

    fn random_order_id(&mut self) -> u64 {
        let idx = self.rng.random_range(0..self.active_ids.len());
        self.active_ids[idx]
    }

    fn cancel(&mut self) -> Vec<u8> {
        let id = self.random_order_id();
        let cancelled_shares = {
            let idx = self.id_to_index[&id];
            let order = &mut self.orders[idx];
            let shares = self.rng.random_range(1..=order.shares);
            order.shares -= shares;
            shares
        };
        if self.orders[self.id_to_index[&id]].shares == 0 {
            self.remove(id);
        }

        let mut buf = vec![0u8; 23];
        buf[0] = b'X';
        w16(&mut buf, 1, self.cfg.stock_locate);
        w16(&mut buf, 3, self.tracking);
        self.tracking = self.tracking.wrapping_add(1);
        w48(&mut buf, 5, self.next_ts());
        w64(&mut buf, 11, id);
        w32(&mut buf, 19, cancelled_shares);
        buf
    }

    fn delete(&mut self) -> Vec<u8> {
        let id = self.random_order_id();
        self.remove(id);

        let mut buf = vec![0u8; 19];
        buf[0] = b'D';
        w16(&mut buf, 1, self.cfg.stock_locate);
        w16(&mut buf, 3, self.tracking);
        self.tracking = self.tracking.wrapping_add(1);
        w48(&mut buf, 5, self.next_ts());
        w64(&mut buf, 11, id);
        buf
    }

    fn replace(&mut self) -> Vec<u8> {
        let id = self.random_order_id();
        let side = self.orders[self.id_to_index[&id]].side;
        self.remove(id);

        let new_id = self.next_ref;
        self.next_ref += 1;
        let price = self.add_price(side);
        let shares = self.rng.random_range(1..=1000);
        self.insert(
            new_id,
            ActiveOrder {
                side,
                price,
                shares,
            },
        );

        let mut buf = vec![0u8; 35];
        buf[0] = b'U';
        w16(&mut buf, 1, self.cfg.stock_locate);
        w16(&mut buf, 3, self.tracking);
        self.tracking = self.tracking.wrapping_add(1);
        w48(&mut buf, 5, self.next_ts());
        w64(&mut buf, 11, id);
        w64(&mut buf, 19, new_id);
        w32(&mut buf, 27, shares);
        w32(&mut buf, 31, price);
        buf
    }

    fn execute(&mut self, with_price: bool) -> Vec<u8> {
        let id = self.random_order_id();
        let (executed_shares, order_price) = {
            let idx = self.id_to_index[&id];
            let order = &mut self.orders[idx];
            let price = order.price;
            let shares = self.rng.random_range(1..=order.shares);
            order.shares -= shares;
            (shares, price)
        };
        if self.orders[self.id_to_index[&id]].shares == 0 {
            self.remove(id);
        }

        let match_number = self.next_match;
        self.next_match += 1;

        if with_price {
            let mut buf = vec![0u8; 36];
            buf[0] = b'C';
            w16(&mut buf, 1, self.cfg.stock_locate);
            w16(&mut buf, 3, self.tracking);
            self.tracking = self.tracking.wrapping_add(1);
            w48(&mut buf, 5, self.next_ts());
            w64(&mut buf, 11, id);
            w32(&mut buf, 19, executed_shares);
            w64(&mut buf, 23, match_number);
            buf[31] = b'Y';
            w32(&mut buf, 32, order_price);
            buf
        } else {
            let mut buf = vec![0u8; 31];
            buf[0] = b'E';
            w16(&mut buf, 1, self.cfg.stock_locate);
            w16(&mut buf, 3, self.tracking);
            self.tracking = self.tracking.wrapping_add(1);
            w48(&mut buf, 5, self.next_ts());
            w64(&mut buf, 11, id);
            w32(&mut buf, 19, executed_shares);
            w64(&mut buf, 23, match_number);
            buf
        }
    }

    fn pick_type(&mut self) -> u8 {
        let w = &self.cfg.weights;
        let total = w.add_no_mpid
            + w.add_with_mpid
            + w.cancel
            + w.delete
            + w.replace
            + w.execute
            + w.execute_with_price;
        let mut r = self.rng.random_range(0..total);
        for (weight, ty) in [
            (w.add_no_mpid, b'A'),
            (w.add_with_mpid, b'F'),
            (w.cancel, b'X'),
            (w.delete, b'D'),
            (w.replace, b'U'),
            (w.execute, b'E'),
            (w.execute_with_price, b'C'),
        ] {
            if r < weight {
                return ty;
            }
            r -= weight;
        }
        b'A'
    }
}

pub fn generate(cfg: &SyntheticConfig) -> Feed {
    let mut g = Generator::new(cfg.clone());

    let mut prefill = Vec::with_capacity(cfg.prefill_orders);
    for _ in 0..cfg.prefill_orders {
        let with_mpid = g.rng.random_bool(0.05);
        prefill.push(g.add(with_mpid));
    }

    let mut messages = Vec::with_capacity(cfg.message_count);
    for _ in 0..cfg.message_count {
        let mut ty = g.pick_type();
        if g.active_ids.is_empty() && ty != b'A' && ty != b'F' {
            ty = b'A';
        }
        messages.push(match ty {
            b'A' => g.add(false),
            b'F' => g.add(true),
            b'X' => g.cancel(),
            b'D' => g.delete(),
            b'U' => g.replace(),
            b'E' => g.execute(false),
            b'C' => g.execute(true),
            _ => unreachable!(),
        });
    }

    Feed {
        prefill,
        messages,
        final_active_orders: g.active_ids.len(),
    }
}

fn w16(buf: &mut [u8], offset: usize, val: u16) {
    buf[offset..offset + 2].copy_from_slice(&val.to_be_bytes());
}

fn w32(buf: &mut [u8], offset: usize, val: u32) {
    buf[offset..offset + 4].copy_from_slice(&val.to_be_bytes());
}

fn w48(buf: &mut [u8], offset: usize, val: u64) {
    let bytes = val.to_be_bytes();
    buf[offset..offset + 6].copy_from_slice(&bytes[2..8]);
}

fn w64(buf: &mut [u8], offset: usize, val: u64) {
    buf[offset..offset + 8].copy_from_slice(&val.to_be_bytes());
}
