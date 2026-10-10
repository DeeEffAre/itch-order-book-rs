# ITCH Order Book Engine (Rust)

A single-threaded Rust order book engine that builds and maintains book state
from a Nasdaq ITCH 5.0 message feed. It processes `Add`/`Cancel`/`Delete`/
`Replace`/`Execute`/`System Event` messages, maintains one book per stock locate,
tracks best bid/ask, and rejects orders outside the active price window. It is a
book builder, not a matcher: the ITCH feed already carries executions, so there
is no crossing or trade generation.

This is the original implementation; it was later ported to C++
([`itch-order-book-cpp`](https://github.com/DeeEffAre/itch-order-book-cpp)).

## Performance

Criterion, median of 100 samples with 3s warm-up and 5s measurement; per-iteration
setup (template clone / book pre-creation) is excluded via `iter_batched`.
Measured on an AMD Ryzen 7 7840HS under WSL2. Reproduce with:

```bash
cargo bench --bench order_book_ops
cargo bench --bench itch_replay
```

Synthetic feeds (deterministic, seed 42). `BTreeMap` and `Segmented array` are the
earlier designs kept for reference (see [Design evolution](#design-evolution)):

| Benchmark                        | Messages  | Time (median) | ns/msg | BTreeMap | Segmented array | Result          |
|----------------------------------|-----------|---------------|--------|----------|-----------------|-----------------|
| `mixed/parse_and_dispatch`       | 1,000,000 | 73.4 ms       | ~73    | ~416     | ~157            | 5.7x vs BTreeMap |
| `parser/parse_and_dispatch_noop` | 1,000,000 | 724.1 µs      | ~0.72  | ~0.7     | ~0.7            | same            |
| `single_type/add`                | 200,000   | 3.66 ms       | ~18    | ~480     | ~139            | 26x vs BTreeMap |
| `single_type/cancel`             | 200,000   | 15.1 ms       | ~76    | ~91      | ~182            | ~same           |
| `single_type/delete`             | 200,000   | 17.8 ms       | ~89    | ~407     | ~178            | 4.6x vs BTreeMap |
| `single_type/replace`            | 200,000   | 35.4 ms       | ~177   | ~897     | ~308            | 5.1x vs BTreeMap |
| `single_type/execute`            | 200,000   | 16.3 ms       | ~81    | ~109     | ~183            | 1.3x vs BTreeMap |
| `depth/shallow`                  | 1,000,000 | 69.1 ms       | ~69    | ~231     | ~144            | 3.3x vs BTreeMap |
| `depth/medium`                   | 1,000,000 | 70.9 ms       | ~71    | ~418     | ~167            | 5.9x vs BTreeMap |
| `depth/deep`                     | 1,000,000 | 73.9 ms       | ~74    | ~1,439   | ~183            | 20x vs BTreeMap |

Real ITCH replay (74 MB subset, 2,535,085 messages, 1,064 locates):

| Benchmark                 | Messages  | Time (median) | ns/msg | msg/s |
|---------------------------|-----------|---------------|--------|-------|
| `replay/book_full_stream` | 2,535,085 | 312.1 ms      | ~123   | ~8.1M |
| `replay/parser_only_noop` | 2,535,085 | 5.05 ms       | ~2.0   | ~501M |

Takeaways: parsing is nearly free (~0.7-2 ns/msg); essentially all cost lives in
the book data structures. Direct indexing plus the bitmap removed the linear scans
that dominated removals, giving the largest gains vs the original BTreeMap design
in `add` (26x) and `depth/deep` (20x).

## Design

- **Fixed price-level arrays + two-level bitmap.** Each book holds two arrays of
  `MAX_PRICE` (100k) `PriceLevel` entries. A 100k-bit L1 bitmap plus a 64-bit L2
  summary gives near-O(1) best bid/ask and next-level search without pointer
  chasing. The bitmaps are heap-allocated (`Box<PriceBitset>`): embedding them
  inline grew `OrderBook` to ~25 KB, pushed hot fields to a later page, and cost
  30-43% on `add`.
- **Price offset sliding instead of shifting.** Levels are indexed by
  `tick = price / TICK_SIZE` rebased against a sliding offset; a new low resets the
  levels and re-places active orders (O(active orders + MAX_PRICE)). The earlier
  `copy_within` approach moved 6.4 MB per cent dropped and was removed.
- **Slab storage + intrusive doubly-linked lists.** Orders live in a single
  `Vec<Order>` slab with a free list; each price level is an intrusive list over
  slab indices. Removal is O(1) (no `VecDeque::retain`). `Order` is 32 bytes
  (timestamp removed from the hot struct).
- **Direct-indexed global order map.** `order_ref -> (locate, slab index)` lives in
  a flat `Vec<OrderLookupInfo>`, bounds-checked lookups for cancel/delete/replace/
  execute. Memory grows with the order-ref space.
- **Per-symbol books.** `Engine` holds `books: Vec<Option<OrderBook>>` indexed by
  stock locate; a book is created lazily on first touch and system events are
  broadcast to all existing books.
- **Zero-copy parser.** Wire structs are `zerocopy` views over the raw buffer;
  big-endian fields are decoded lazily. Dispatch is a macro-generated static match
  on the message type, with no dynamic dispatch; unknown types are ignored.
- **Single-threaded by design.** No synchronization on the hot path. A
  feed-handler to strategy hand-off would be a separate, explicitly measured
  component.

## Design evolution

The current layout is the third iteration:

1. `BTreeMap<Price, VecDeque<Id>>` + `HashMap` order pool: clean, but O(n) removal
   (`retain`) and tree traversal dominated the hot path.
2. Lazy-allocation segmented array: direct price indexing + slab + O(1) unlink,
   segments allocated on first touch.
3. Fixed-array direct mapping + per-symbol `Engine` + price-offset rebase +
   two-level bitmap (current): removes segment arithmetic and turns best-price
   scans into bitmap lookups.

## Layout

```
src/
  itch_parser/   # zero-copy wire structs + static dispatch
  order_book/    # book, price bitmap, slab, order types
  engine/        # per-locate books + global order map
benches/         # Criterion: synthetic feeds + real ITCH replay
tests/           # integration tests (parser, splitter, generator/book parity)
```

## Build & Run

```bash
cargo test
cargo bench --bench order_book_ops
cargo bench --bench itch_replay
```

The replay benchmark reads a length-prefixed Nasdaq ITCH 5.0 binary file. Set
`ITCH_FILE` to override the default `$HOME/data/itch/itch_subset.bin`, or fetch a
subset with `./scripts/fetch_itch_data.sh`.

## Scope & Limitations

- Nasdaq ITCH 5.0 subset only (`A/F/X/D/E/U/C/S`); no MoldUDP64 framing, sequence
  numbers, or gap recovery.
- Book state only: no matching/crossing, no strategy or risk layer, no FIX/OUCH
  gateway.
- Single-threaded; no lock-free hand-off.
- Fixed 100k-tick price window; out-of-band orders are silently dropped.
- Memory: replaying 1,064 locates allocates ~3.4 GB (one fixed-array book each);
  the global map grows with the order-ref space.
- Synthetic benchmark feeds are generated; the replay figures come from a 74 MB
  ITCH subset and will differ on other files.

## Notes

- Measurements under WSL2 vary by roughly ±10% between sessions; treat
  same-session A/B runs as the source of truth and prefer ratios over single runs.
- The parser is at parity with the C++ port (~2 ns/msg), so cross-language
  differences are in the book operations.
