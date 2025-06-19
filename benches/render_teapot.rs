use std::time::Duration;

use criterion::{Criterion, criterion_group, criterion_main};
use minipath::{
    Camera, RenderSettings, Scene,
    geometry::{ScreenSize, WorldPoint, WorldVector},
    render,
    scene::TriangleBvh,
};

fn criterion_benchmark(c: &mut Criterion) {
    let camera = Camera::builder()
        .center(WorldPoint::new(0.0, 2.0, 10.0))
        .forward(WorldVector::new(0.0, 0.0, -1.0))
        .up(WorldVector::new(0.0, 1.0, 0.0))
        .resolution(ScreenSize::new(2048, 1536))
        .film_width(36e-3)
        .focal_length(50e-3)
        .f_number(4.8)
        .focus_distance(10.0)
        .build();
    let settings = RenderSettings {
        tile_size: 64.try_into().unwrap(),
        sample_count: 10.try_into().unwrap(),
    };
    let scene = Scene {
        object: TriangleBvh::with_obj("data/teapot.obj").unwrap(),
    };

    c.bench_function("render_teapot", |b| {
        b.iter_batched(
            || (camera.clone(), settings.clone(), scene.clone()),
            |(camera, settings, scene)| {
                let mut render_progress =
                    render(scene, camera, settings, |_| {}, |_, _| {}).unwrap();
                render_progress.wait();
            },
            criterion::BatchSize::LargeInput,
        )
    });
}

criterion_group! {
    name = benches;
    config = Criterion::default().sample_size(20).measurement_time(Duration::from_secs(60));
    targets = criterion_benchmark
}
criterion_main!(benches);
