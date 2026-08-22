#[path = "../benches/common/itch.rs"]
mod itch;

#[path = "../benches/common/synthetic.rs"]
mod synthetic;

use order_book_matching_engine::engine::Engine;
use order_book_matching_engine::itch_parser::parse_and_dispatch;

fn length_prefix(messages: &[Vec<u8>]) -> Vec<u8> {
    let mut out = Vec::new();
    for m in messages {
        #[allow(clippy::cast_possible_truncation)]
        out.extend_from_slice(&(m.len() as u16).to_be_bytes());
        out.extend_from_slice(m);
    }
    out
}

#[test]
fn splitter_recovers_all_messages() {
    let cfg = synthetic::SyntheticConfig {
        message_count: 10_000,
        prefill_orders: 1_000,
        ..Default::default()
    };
    let feed = synthetic::generate(&cfg);

    let mut messages = feed.prefill;
    messages.extend(feed.messages);

    let buf = length_prefix(&messages);
    let recovered: Vec<&[u8]> = itch::Splitter::new(&buf).collect();

    assert_eq!(recovered.len(), messages.len());
    for (recovered, original) in recovered.iter().zip(&messages) {
        assert_eq!(*recovered, original.as_slice());
    }
}

#[test]
fn splitter_handles_truncated_tail() {
    let buf = [0u8, 36, b'A', 0, 1];
    let recovered: Vec<&[u8]> = itch::Splitter::new(&buf).collect();
    assert!(
        recovered.is_empty(),
        "partial trailing message must be dropped"
    );
}

#[test]
fn count_matches_splitter() {
    let cfg = synthetic::SyntheticConfig {
        message_count: 5_000,
        prefill_orders: 500,
        ..Default::default()
    };
    let feed = synthetic::generate(&cfg);
    let buf = length_prefix(&feed.messages);

    assert_eq!(itch::count_messages(&buf), feed.messages.len());
}

#[test]
fn normalize_locate_rewrites_every_message() {
    let cfg = synthetic::SyntheticConfig {
        message_count: 5_000,
        prefill_orders: 500,
        ..Default::default()
    };
    let feed = synthetic::generate(&cfg);
    let mut buf = length_prefix(&feed.messages);

    itch::normalize_locate(&mut buf, 7);

    for msg in itch::Splitter::new(&buf) {
        assert_eq!(msg[1..3], 7u16.to_be_bytes());
    }
}

#[test]
fn normalized_stream_replays_without_panic() {
    let cfg = synthetic::SyntheticConfig {
        message_count: 20_000,
        prefill_orders: 5_000,
        ..Default::default()
    };
    let feed = synthetic::generate(&cfg);
    let mut buf = length_prefix(&feed.prefill);
    buf.extend(length_prefix(&feed.messages));
    itch::normalize_locate(&mut buf, 7);

    let mut engine = Engine::default();
    for msg in itch::Splitter::new(&buf) {
        parse_and_dispatch(msg, &mut engine);
    }

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
