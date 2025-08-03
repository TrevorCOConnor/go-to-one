use criterion::{criterion_group, criterion_main, Criterion};
use lib::relative_roi;

const TEST_VIDEO_FP: &str = "";
const TEST_ANNOTATION_FP: &str = "";
const TEST_OUTPUT_FP: &str = "";

pub fn whole_benchmark(c: &mut Criterion) {
    c.bench_function("whole", |b| b.iter(|| run(TEST_VIDEO_FP, TEST_ANNOTATION_FP, TEST_OUTPUT_FP, None)));
}

criterion_group!(benches, whole_benchmark);
criterion_main!(benches);
