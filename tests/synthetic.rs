#[path = "../benches/common/synthetic.rs"]
mod synthetic;

use order_book_matching_engine::engine::Engine;
use order_book_matching_engine::itch_parser::parse_and_dispatch;

fn replay(feed: &synthetic::Feed) -> Engine {
    let mut engine = Engine::default();
    for m in &feed.prefill {
        parse_and_dispatch(m, &mut engine);
    }
    for m in &feed.messages {
        parse_and_dispatch(m, &mut engine);
    }
    engine
}

#[test]
fn generated_stream_matches_book_state() {
    let cfg = synthetic::SyntheticConfig {
        message_count: 50_000,
        prefill_orders: 10_000,
        ..Default::default()
    };
    let feed = synthetic::generate(&cfg);

    let engine = replay(&feed);

    assert_eq!(
        total_active_orders(&engine),
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

    let engine = replay(&feed);

    let in_queues = count_in_queues(&engine);
    assert_eq!(in_queues, total_active_orders(&engine));
}

fn count_in_queues(engine: &Engine) -> usize {
    engine
        .books
        .iter()
        .flatten()
        .map(|book| {
            book.bids
                .iter()
                .map(|level| level.count as usize)
                .sum::<usize>()
                + book
                    .asks
                    .iter()
                    .map(|level| level.count as usize)
                    .sum::<usize>()
        })
        .sum()
}

fn total_active_orders(engine: &Engine) -> usize {
    engine
        .books
        .iter()
        .flatten()
        .map(|book| book.order_pool.len())
        .sum()
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

        let engine = replay(&feed);

        assert_eq!(
            total_active_orders(&engine),
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

#[test]
fn engine_tracks_order_locations_in_global_map() {
    use order_book_matching_engine::order_book::SlabIndex;

    let mut engine = Engine::default();

    let mut add = [0u8; 36];
    add[0] = b'A';
    add[1..3].copy_from_slice(&100u16.to_be_bytes());
    add[11..19].copy_from_slice(&5u64.to_be_bytes());
    add[19] = b'B';
    add[20..24].copy_from_slice(&100u32.to_be_bytes());
    add[24..32].copy_from_slice(b"AAPL    ");
    add[32..36].copy_from_slice(&500_000u32.to_be_bytes());
    parse_and_dispatch(&add, &mut engine);

    let info = engine.global_map.map[5];
    assert_eq!(info.locate, 100);
    assert!(!info.slab_idx.is_null());
    assert_eq!(total_active_orders(&engine), 1);

    // delete the order -> map entry cleared
    let mut del = [0u8; 19];
    del[0] = b'D';
    del[1..3].copy_from_slice(&100u16.to_be_bytes());
    del[11..19].copy_from_slice(&5u64.to_be_bytes());
    parse_and_dispatch(&del, &mut engine);

    assert!(engine.global_map.map[5].slab_idx == SlabIndex::NULL);
    assert_eq!(total_active_orders(&engine), 0);
}

#[test]
fn unknown_order_messages_are_ignored() {
    let mut engine = Engine::default();

    let mut del = [0u8; 19];
    del[0] = b'D';
    del[1..3].copy_from_slice(&100u16.to_be_bytes());
    del[11..19].copy_from_slice(&999_999_999u64.to_be_bytes());
    parse_and_dispatch(&del, &mut engine);

    let mut cancel = [0u8; 23];
    cancel[0] = b'X';
    cancel[1..3].copy_from_slice(&100u16.to_be_bytes());
    cancel[11..19].copy_from_slice(&999_999_999u64.to_be_bytes());
    parse_and_dispatch(&cancel, &mut engine);

    let mut exe = [0u8; 31];
    exe[0] = b'E';
    exe[1..3].copy_from_slice(&100u16.to_be_bytes());
    exe[11..19].copy_from_slice(&999_999_999u64.to_be_bytes());
    parse_and_dispatch(&exe, &mut engine);

    // no books created, no map entries
    assert_eq!(total_active_orders(&engine), 0);
    assert!(engine.global_map.map.is_empty());
}
