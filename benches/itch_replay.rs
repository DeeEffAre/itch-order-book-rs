mod common;

use criterion::{BatchSize, Criterion, criterion_group, criterion_main};
use order_book_matching_engine::engine::Engine;
use order_book_matching_engine::itch_parser::{handler::MessageHandler, parse_and_dispatch};
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
    let data = itch::load(&path);
    let msg_count = itch::count_messages(&data);
    let locates = itch::distinct_locates(&data);
    eprintln!(
        "loaded {path}: {} bytes, {msg_count} messages, {} locates",
        data.len(),
        locates.len()
    );

    c.bench_function("replay/book_full_stream", |b| {
        b.iter_batched(
            || {
                // pre-create one book per locate so first-touch allocation stays untimed
                let mut engine = Engine::default();
                for &locate in &locates {
                    engine.inline_get_book(locate);
                }
                engine
            },
            |mut engine| {
                for msg in Splitter::new(&data) {
                    parse_and_dispatch(black_box(msg), &mut engine);
                }
                black_box(engine)
            },
            BatchSize::LargeInput,
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
