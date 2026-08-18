Basic implementation for a small section of an order book matching engine for NASDAQ ITCH protocol.

# Order Book

```rust
pub struct OrderBook {
    pub segments_bids: Vec<Option<Box<[PriceLevel]>>>,
    pub segments_asks: Vec<Option<Box<[PriceLevel]>>>,
    pub id_map: FxHashMap<order::Id, SlabIndex>,
    pub order_pool: Slab,
    pub best_bid: u64,
    pub best_ask: u64,
}
```

OrderBook utilizes lazy segmented price arrays.

# Benchmark

| Bench                            | Messages  | Time (median) | Time per message (ns) | Old results (ns/msg) | Change          |
|----------------------------------|-----------|---------------|-----------------------|----------------------|-----------------|
| `mixed/parse_and_dispatch`       | 1,000,000 | 156.58 ms     | **~157**              | ~416                 | **2.6x faster** |
| `parser/parse_and_dispatch_noop` | 1,000,000 | 711.9 µs      | ~0.7                  | ~0.7                 | same            |
| `single_type/add`                | 200,000   | 27.80 ms      | ~139                  | ~480                 | **3.5x faster** |
| `single_type/cancel`             | 200,000   | 36.46 ms      | ~182                  | ~91                  | **2x slower**   |
| `single_type/delete`             | 200,000   | 35.60 ms      | ~178                  | ~407                 | **2.3x faster** |
| `single_type/replace`            | 200,000   | 61.53 ms      | ~308                  | ~897                 | **2.9x faster** |
| `single_type/execute`            | 200,000   | 36.68 ms      | ~183                  | ~109                 | **1.7x slower** |
| `depth/shallow`                  | 1,000,000 | 143.7 ms      | ~144                  | ~231                 | **1.6x faster** |
| `depth/medium`                   | 1,000,000 | 166.5 ms      | ~167                  | ~418                 | **2.5x faster** |
| `depth/deep`                     | 1,000,000 | 182.5 ms      | ~183                  | ~1,439               | **7.9x faster** |
