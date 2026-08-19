mod common;

use criterion::{BatchSize, Criterion, criterion_group, criterion_main};
use order_book_matching_engine::engine::Engine;
use order_book_matching_engine::itch_parser::{handler::MessageHandler, parse_and_dispatch};
use std::hint::black_box;

use common::synthetic::{self, SyntheticConfig, Weights};

const ZERO_WEIGHTS: Weights = Weights {
    add_no_mpid: 0,
    add_with_mpid: 0,
    cancel: 0,
    delete: 0,
    replace: 0,
    execute: 0,
    execute_with_price: 0,
};

/// Replays the feed's prefill into a fresh book (setup, not timed), then times
/// the replay of the measured message stream against that deep book.
fn bench_mixed_stream(c: &mut Criterion, id: &str, cfg: &SyntheticConfig) {
    let feed = synthetic::generate(cfg);
    c.bench_function(id, |b| {
        b.iter_batched(
            || {
                let mut engine = Engine::default();
                for m in &feed.prefill {
                    parse_and_dispatch(m, &mut engine);
                }
                engine
            },
            |mut engine| {
                for m in &feed.messages {
                    parse_and_dispatch(black_box(m), &mut engine);
                }
                black_box(engine)
            },
            BatchSize::SmallInput,
        );
    });
}

struct NoopHandler;

impl MessageHandler for NoopHandler {}

fn bench_ops(c: &mut Criterion) {
    let default_cfg = SyntheticConfig::default();

    bench_mixed_stream(c, "mixed/parse_and_dispatch", &default_cfg);

    let feed = synthetic::generate(&default_cfg);
    c.bench_function("parser/parse_and_dispatch_noop", |b| {
        b.iter(|| {
            let mut handler = NoopHandler;
            for m in &feed.messages {
                parse_and_dispatch(black_box(m), &mut handler);
            }
            black_box(&handler);
        });
    });

    for (name, weights) in [
        (
            "add",
            Weights {
                add_no_mpid: 1,
                ..ZERO_WEIGHTS
            },
        ),
        (
            "cancel",
            Weights {
                cancel: 1,
                ..ZERO_WEIGHTS
            },
        ),
        (
            "delete",
            Weights {
                delete: 1,
                ..ZERO_WEIGHTS
            },
        ),
        (
            "replace",
            Weights {
                replace: 1,
                ..ZERO_WEIGHTS
            },
        ),
        (
            "execute",
            Weights {
                execute: 1,
                ..ZERO_WEIGHTS
            },
        ),
    ] {
        let cfg = SyntheticConfig {
            message_count: 200_000,
            prefill_orders: 300_000,
            weights,
            ..default_cfg.clone()
        };
        bench_mixed_stream(c, &format!("single_type/{name}"), &cfg);
    }

    for (name, half_band) in [
        ("shallow", 1_000u32),
        ("medium", 30_000u32),
        ("deep", 250_000u32),
    ] {
        let cfg = SyntheticConfig {
            half_band,
            ..default_cfg.clone()
        };
        bench_mixed_stream(c, &format!("depth/{name}"), &cfg);
    }
}

criterion_group!(benches, bench_ops);
criterion_main!(benches);
