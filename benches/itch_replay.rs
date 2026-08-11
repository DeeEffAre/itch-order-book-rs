mod common;

use criterion::{BatchSize, Criterion, criterion_group, criterion_main};
use order_book_matching_engine::itch_parser::{handler::MessageHandler, parse_and_dispatch};
use order_book_matching_engine::order_book::OrderBook;
use std::hint::black_box;

use common::itch::{self, Splitter};

fn default_path() -> String {
    let home = std::env::var("HOME").unwrap_or_else(|_| ".".to_string());
    format!("{home}/data/itch/itch_subset.bin")
}

struct NoopHandler;

impl MessageHandler for NoopHandler {}

fn bench_replay(c: &mut Criterion) {
    let path = std::env::var("ITCH_FILE").unwrap_or_else(|_| default_path());
    let mut data = itch::load(&path);
    itch::normalize_locate(&mut data, 1);
    let msg_count = itch::count_messages(&data);
    eprintln!("loaded {path}: {} bytes, {msg_count} messages", data.len());

    c.bench_function("replay/book_full_stream", |b| {
        b.iter_batched(
            OrderBook::default,
            |mut book| {
                for msg in Splitter::new(&data) {
                    parse_and_dispatch(black_box(msg), &mut book);
                }
                black_box(book)
            },
            BatchSize::SmallInput,
        );
    });

    c.bench_function("replay/parser_only_noop", |b| {
        b.iter(|| {
            let mut handler = NoopHandler;
            for msg in Splitter::new(&data) {
                parse_and_dispatch(black_box(msg), &mut handler);
            }
            black_box(&handler);
        });
    });
}

criterion_group!(benches, bench_replay);
criterion_main!(benches);
