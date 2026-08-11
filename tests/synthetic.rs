#[path = "../benches/common/synthetic.rs"]
mod synthetic;

use order_book_matching_engine::itch_parser::parse_and_dispatch;
use order_book_matching_engine::order_book::OrderBook;

fn replay(feed: &synthetic::Feed) -> OrderBook {
    let mut book = OrderBook::default();
    for m in &feed.prefill {
        parse_and_dispatch(m, &mut book);
    }
    for m in &feed.messages {
        parse_and_dispatch(m, &mut book);
    }
    book
}

#[test]
fn generated_stream_matches_book_state() {
    let cfg = synthetic::SyntheticConfig {
        message_count: 50_000,
        prefill_orders: 10_000,
        ..Default::default()
    };
    let feed = synthetic::generate(&cfg);

    let book = replay(&feed);

    assert_eq!(
        book.order_pool.len(),
        feed.final_active_orders,
        "generator pool and book pool diverged"
    );
}

#[test]
fn book_queues_are_consistent_after_replay() {
    let cfg = synthetic::SyntheticConfig {
        message_count: 50_000,
        prefill_orders: 10_000,
        ..Default::default()
    };
    let feed = synthetic::generate(&cfg);

    let book = replay(&feed);

    let in_queues: usize = book
        .bids
        .values()
        .map(std::collections::VecDeque::len)
        .sum::<usize>()
        + book
            .asks
            .values()
            .map(std::collections::VecDeque::len)
            .sum::<usize>();
    assert_eq!(in_queues, book.order_pool.len());
}

#[test]
fn single_type_streams_keep_pool_alive() {
    for (name, weights) in [
        (
            "cancel",
            synthetic::Weights {
                add_no_mpid: 0,
                add_with_mpid: 0,
                cancel: 1,
                delete: 0,
                replace: 0,
                execute: 0,
                execute_with_price: 0,
            },
        ),
        (
            "delete",
            synthetic::Weights {
                add_no_mpid: 0,
                add_with_mpid: 0,
                cancel: 0,
                delete: 1,
                replace: 0,
                execute: 0,
                execute_with_price: 0,
            },
        ),
        (
            "replace",
            synthetic::Weights {
                add_no_mpid: 0,
                add_with_mpid: 0,
                cancel: 0,
                delete: 0,
                replace: 1,
                execute: 0,
                execute_with_price: 0,
            },
        ),
        (
            "execute",
            synthetic::Weights {
                add_no_mpid: 0,
                add_with_mpid: 0,
                cancel: 0,
                delete: 0,
                replace: 0,
                execute: 1,
                execute_with_price: 0,
            },
        ),
    ] {
        let cfg = synthetic::SyntheticConfig {
            message_count: 20_000,
            prefill_orders: 100_000,
            weights,
            ..Default::default()
        };
        let feed = synthetic::generate(&cfg);

        let book = replay(&feed);

        assert_eq!(
            book.order_pool.len(),
            feed.final_active_orders,
            "{name}: generator pool and book pool diverged"
        );
    }
}

#[test]
fn message_len_table_is_exhaustive_for_generated_types() {
    let cfg = synthetic::SyntheticConfig {
        message_count: 10_000,
        prefill_orders: 1_000,
        ..Default::default()
    };
    for msg in &synthetic::generate(&cfg).messages {
        let ty = msg[0];
        assert_eq!(
            synthetic::message_len(ty),
            Some(msg.len()),
            "message type {ty} has unexpected length {}",
            msg.len()
        );
    }
}
