Basic implementation for a small section of an order book matching engine for NASDAQ ITCH protocol.

# Order Book

```rust
pub struct GlobalMap {
    pub map: Vec<OrderLookupInfo>,
}

pub struct Engine {
    pub books: Vec<Option<OrderBook>>,
    pub global_map: GlobalMap,
}

pub struct OrderBook {
    pub price_offset: Option<u64>,
    pub bids: Vec<PriceLevel>,
    pub asks: Vec<PriceLevel>,
    pub order_pool: Slab,
    pub best_bid: u32,
    pub best_ask: u32,
}
```

OrderBook is in tick indexed direct mapping format. With Engine implementation every stock has its own OrderBook and finally with GlobalMap the engine can perform cross symbol lookup globally

# Benchmark

| Bench                            | Messages  | Time (median) | ns/msg | report1 (BTreeMap) | report2 (segmented) | Result              |
|----------------------------------|-----------|---------------|--------|--------------------|---------------------|---------------------|
| `mixed/parse_and_dispatch`       | 1,000,000 | 67.2 ms       | ~67    | ~416               | ~157                | 6.2x vs report1     |
| `parser/parse_and_dispatch_noop` | 1,000,000 | 720.2 µs      | ~0.72  | ~0.7               | ~0.7                | same                |
| `single_type/add`                | 200,000   | 3.38 ms       | ~17    | ~480               | ~139                | 28x vs report1      |
| `single_type/cancel`             | 200,000   | 15.9 ms       | ~79    | ~91                | ~182                | same as report1     |
| `single_type/delete`             | 200,000   | 19.3 ms       | ~97    | ~407               | ~178                | 4.2x vs report1     |
| `single_type/replace`            | 200,000   | 31.7 ms       | ~158   | ~897               | ~308                | 5.7x vs report1     |
| `single_type/execute`            | 200,000   | 17.1 ms       | ~86    | ~109               | ~183                | faster than report1 |
| `depth/shallow`                  | 1,000,000 | 67.5 ms       | ~67    | ~231               | ~144                | 3.4x vs report1     |
| `depth/medium`                   | 1,000,000 | 67.5 ms       | ~68    | ~418               | ~167                | 6.1x vs report1     |
| `depth/deep`                     | 1,000,000 | 72.3 ms       | ~72    | ~1,439             | ~183                | 20x vs report1      |

| Bench                     | Time    | ns/msg | msg/s |
|---------------------------|---------|--------|-------|
| `replay/book_full_stream` | 1.075 s | ~424   | ~2.4M |
| `replay/parser_only_noop` | 5.05 ms | ~2.0   | ~497M |
