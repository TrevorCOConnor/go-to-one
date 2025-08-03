use std::time::Duration;

use criterion::{criterion_group, criterion_main, Criterion};
use lib::relative_roi::RelativeRoi;
use opencv::core::{Size, UMat};
use overlay::hero_display::DisplayHeroManager;

const HERO1_NAME: &str = "Maxx 'The Hype' Nitro";
const HERO2_NAME: &str = "Rhinar, Reckless Rampage";

fn display_heroes_for(frames: u64) {
    let mut dhm = DisplayHeroManager::new_def(HERO1_NAME, HERO2_NAME).unwrap();
    let hero1_rel_roi = RelativeRoi::build_def(0.0, 0.0, 0.5, 1.0, None, None).unwrap();
    let hero2_rel_roi = RelativeRoi::build_def(0.5, 0.0, 0.5, 1.0, None, None).unwrap();

    for _ in 0..frames {
        let mut frame = UMat::new_size_def(Size::new(850, 600), 0).unwrap();
        dhm.display_heroes(&mut frame, hero1_rel_roi, hero2_rel_roi, &overlay::TurnPlayer::One, None).unwrap();
    }
}

pub fn display_heroes_benchmark(c: &mut Criterion) {
    let mut group = c.benchmark_group("I dunno");
    group.sample_size(10);
    group.measurement_time(Duration::new(8, 0));
    group.bench_function("display_heroes", |b| b.iter(|| display_heroes_for(10)));
    group.finish();
}

criterion_group!(benches, display_heroes_benchmark);
criterion_main!(benches);
