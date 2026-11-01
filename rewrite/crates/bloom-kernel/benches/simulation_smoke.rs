use std::hint::black_box;

use bloom_kernel::simulation::{run_replay, ReplayCase};
use criterion::{criterion_group, criterion_main, Criterion};

const REPLAY_FIXTURE: &str = include_str!("../../../testdata/replays/empty-kernel-smoke.sim");

fn simulation_replay_smoke(criterion: &mut Criterion) {
    let replay = ReplayCase::parse(REPLAY_FIXTURE).expect("checked-in replay fixture is valid");
    criterion.bench_function("empty_kernel_replay_smoke", |bencher| {
        bencher.iter(|| {
            black_box(
                run_replay(black_box(&replay)).expect("replay remains deterministic and valid"),
            )
        });
    });
}

criterion_group!(benches, simulation_replay_smoke);
criterion_main!(benches);
