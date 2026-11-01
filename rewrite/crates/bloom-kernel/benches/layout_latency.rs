// Wall-clock access is confined to this external measurement harness.
#![allow(clippy::disallowed_methods)]

use std::{
    hint::black_box,
    time::{Duration, Instant},
};

use bloom_kernel::update::{update, view, Event, Model};

const LINE_COUNT: usize = 10_000;
const WARMUP_SAMPLES: usize = 100;
const MEASURED_SAMPLES: usize = 500;
const KEYPRESS_BUDGET: Duration = Duration::from_millis(8);

fn main() {
    let text = (0..LINE_COUNT)
        .map(|line| format!("synthetic note line {line:05}"))
        .collect::<Vec<_>>()
        .join("\n");
    let mut model = Model::new(&text);

    for sample in 0..WARMUP_SAMPLES {
        sample_transition(&mut model, sample % 2 == 0);
    }

    let mut samples = (0..MEASURED_SAMPLES)
        .map(|sample| sample_transition(&mut model, sample % 2 == 0))
        .collect::<Vec<_>>();
    samples.sort_unstable();
    let p50 = samples[samples.len() / 2];
    let p99 = samples[(samples.len() * 99).div_ceil(100) - 1];

    println!(
        "M1 single-editor line split/merge keypress-to-frame (10k lines): p50={p50:?}, p99={p99:?}, budget={KEYPRESS_BUDGET:?}"
    );
    assert!(
        p99 < KEYPRESS_BUDGET,
        "10k-line single-editor p99 exceeded the necessary 8 ms budget: {p99:?}"
    );
}

fn sample_transition(model: &mut Model, insert: bool) -> Duration {
    let started = Instant::now();
    let event = if insert {
        Event::InsertText("\n".to_owned())
    } else {
        Event::Backspace
    };
    update(model, event, Duration::ZERO).expect("synthetic keystroke succeeds");
    black_box(view(model));
    started.elapsed()
}
