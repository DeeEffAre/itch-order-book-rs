Basic implementation for a small section of an order book matching engine for NASDAQ ITCH protocol.

# Order Book

```rust
pub struct OrderBook {
    pub bids: BTreeMap<Reverse<order::Price>, VecDeque<order::Id>>,
    pub asks: BTreeMap<order::Price, VecDeque<order::Id>>,
    pub order_pool: HashMap<order::Id, order::Order>,
}
```

for the time being buy and sell orders are hold in `OrderBook` within `BTreeMap` and `VecDeque` from Rust's `std::collections`.

# Benchmark

| Benchmark                      | Messages  | Time (median) | Time per message |
|--------------------------------|-----------|---------------|------------------|
| mixed/parse_and_dispatch       | 1,000,000 | 390.1 ms      | 390 ns           |
| parser/parse_and_dispatch_noop | 1,000,000 | 696.2 µs      | 0.7 ns           |
| single_type/add                | 200,000   | 97.4 ms       | 487 ns           |
| single_type/cancel             | 200,000   | 18.9 ms       | 94 ns            |
| single_type/delete             | 200,000   | 87.0 ms       | 435 ns           |
| single_type/replace            | 200,000   | 162.3 ms      | 812 ns           |
| single_type/execute            | 200,000   | 18.9 ms       | 95 ns            |
| depth/shallow                  | 1,000,000 | 206.2 ms      | 206 ns           |
| depth/medium                   | 1,000,000 | 382.4 ms      | 382 ns           |
| depth/deep                     | 1,000,000 | 561.7 ms      | 562 ns           |
