//! `changes_bounded` checks each row against `max_bytes` before the row joins
//! the batch, so its peak allocation stays a small multiple of `max_bytes` even
//! when the whole batch would be far larger. A counting global allocator
//! measures that peak, so this test has its own binary and runs alone in it.
use buffa::Message;
use buffa_types::google::protobuf::Timestamp;
use evm_dex::{changes_bounded, pb, Encoding};
use std::alloc::{GlobalAlloc, Layout, System};
use std::sync::atomic::{AtomicUsize, Ordering};
use substreams::pb::substreams::Clock;

/// Counts live heap bytes and their high-water mark.
struct Counting;

static LIVE: AtomicUsize = AtomicUsize::new(0);
static PEAK: AtomicUsize = AtomicUsize::new(0);

fn grow(bytes: usize) {
    let live = LIVE.fetch_add(bytes, Ordering::SeqCst) + bytes;
    PEAK.fetch_max(live, Ordering::SeqCst);
}

fn shrink(bytes: usize) {
    LIVE.fetch_sub(bytes, Ordering::SeqCst);
}

// SAFETY: every call forwards to `System` unchanged; the counters only observe.
unsafe impl GlobalAlloc for Counting {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        let ptr = unsafe { System.alloc(layout) };
        if !ptr.is_null() {
            grow(layout.size());
        }
        ptr
    }

    unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
        let ptr = unsafe { System.alloc_zeroed(layout) };
        if !ptr.is_null() {
            grow(layout.size());
        }
        ptr
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        unsafe { System.dealloc(ptr, layout) };
        shrink(layout.size());
    }

    unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
        // A moving realloc briefly holds both blocks: count the new one first.
        grow(new_size);
        let new = unsafe { System.realloc(ptr, layout, new_size) };
        shrink(if new.is_null() { new_size } else { layout.size() });
        new
    }
}

#[global_allocator]
static ALLOCATOR: Counting = Counting;

fn clock() -> Clock {
    Clock {
        id: "ab".repeat(32),
        number: 23_000_000,
        timestamp: buffa::MessageField::some(Timestamp {
            seconds: 1_760_000_042,
            ..Default::default()
        }),
    }
}

fn log(data: Vec<u8>) -> pb::Log {
    pb::Log {
        address: vec![0x10; 20],
        topics: vec![vec![0xdd; 32]],
        data,
        log: Some(pb::log::Log::from(pb::Swap {
            protocol: pb::Protocol::UniswapV3.into(),
            pool: vec![0x10; 20],
            input_token: vec![0x70; 20],
            input_amount: "1000".into(),
            output_token: vec![0x71; 20],
            output_amount: "990".into(),
            ..Default::default()
        })),
        ..Default::default()
    }
}

/// A swap with no address, topics or amounts: about the smallest input that
/// writes a row, whose short fields cost more memory per encoded byte.
fn empty_log(data: Vec<u8>) -> pb::Log {
    pb::Log {
        data,
        log: Some(pb::log::Log::from(pb::Swap::default())),
        ..Default::default()
    }
}

/// The tightest `max_bytes` the input check accepts for `events`.
fn tightest(events: &pb::Events) -> usize {
    events.try_encoded_len().unwrap() as usize + clock().try_encoded_len().unwrap() as usize
}

/// As many rows of `small` as fit within the tightest `max_bytes`, then one
/// row whose 1 MiB of log data is most of the input: the retained rows fill
/// the budget before that row is built.
fn filled(small: fn(Vec<u8>) -> pb::Log) -> pb::Events {
    let events = |rows: usize, candidate: bool| {
        let mut logs = vec![small(vec![]); rows];
        if candidate {
            logs.push(small(vec![0x5a; 1024 * 1024]));
        }
        pb::Events {
            transactions: vec![pb::Transaction {
                hash: vec![0x11; 32],
                logs,
                ..Default::default()
            }],
        }
    };
    // The rows, with the `blocks` row, fit in the budget of the whole block.
    let fits = |rows: usize| changes_bounded(&clock(), &events(rows, false), Encoding::Hex, rows.max(1), tightest(&events(rows, true))).is_ok();
    let mut rows = 1;
    while fits(2 * rows) {
        rows *= 2;
    }
    let (mut low, mut high) = (rows, 2 * rows);
    while high - low > 1 {
        let middle = (low + high) / 2;
        if fits(middle) {
            low = middle;
        } else {
            high = middle;
        }
    }
    events(low, true)
}

/// The peak extra heap bytes of `changes_bounded` for `events` at the tightest
/// `max_bytes` its input check accepts, and that `max_bytes`.
fn peak_of_rejected(events: &pb::Events) -> (usize, usize) {
    let clock = clock();
    let max_bytes = tightest(events);
    let logs = events.transactions.iter().map(|tx| tx.logs.len()).sum();
    let before = LIVE.load(Ordering::SeqCst);
    PEAK.store(before, Ordering::SeqCst);
    let error = changes_bounded(&clock, events, Encoding::Hex, logs, max_bytes).unwrap_err();
    let peak = PEAK.load(Ordering::SeqCst) - before;
    assert!(error.to_string().contains("output capacity exceeded"), "{error}");
    (peak, max_bytes)
}

#[test]
fn rejected_batches_peak_within_a_small_multiple_of_max_bytes() {
    let large_field = |logs: Vec<pb::Log>| pb::Events {
        transactions: vec![pb::Transaction {
            hash: vec![0x11; 32],
            gas_price: "1".repeat(64 * 1024),
            logs,
            ..Default::default()
        }],
    };
    // Every row repeats a 64 KiB transaction field: the whole batch would
    // retain 256 MiB, about 400 times the encoded input.
    let repeated = large_field(vec![log(vec![]); 4096]);
    // One row whose hex-encoded data is twice the encoded input.
    let expanded = pb::Events {
        transactions: vec![pb::Transaction {
            hash: vec![0x11; 32],
            logs: vec![log(vec![0x5a; 4 * 1024 * 1024])],
            ..Default::default()
        }],
    };
    // Rows up to the budget, then that hex-expanded row.
    let mut both = vec![log(vec![]); 63];
    both.push(log(vec![0x5a; 4 * 1024 * 1024]));
    let both = large_field(both);
    // Measured: about 1.1x, 4.0x (the hex string is built twice while its row
    // is set) and 5.0x; then 7.4x and 8.4x, where minimal or empty rows,
    // holding 3.8x and 4.4x their encoded size, fill the budget before such a
    // row. The bound is the README's worst case. Checking the budget only
    // after the loop retains the whole repeated batch instead, about 450x.
    let shapes = [
        ("repeated", repeated),
        ("expanded", expanded),
        ("both", both),
        ("filled", filled(log)),
        ("filled_empty", filled(empty_log)),
    ];
    for (name, events) in shapes {
        let (peak, max_bytes) = peak_of_rejected(&events);
        assert!(peak <= 9 * max_bytes, "{name}: peak {peak} bytes for max_bytes {max_bytes}");
    }
}
