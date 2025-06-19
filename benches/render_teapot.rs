use criterion::{Criterion, criterion_group, criterion_main};
use minipath::{
    Camera, RenderSettings, Scene,
    geometry::{ScreenSize, WorldPoint, WorldVector},
    render,
    scene::TriangleBvh,
};

fn criterion_benchmark(c: &mut Criterion) {
    let camera = Camera::new(
        WorldPoint::new(0.0, -5.0, 1.0),
        WorldVector::new(0.0, 1.0, 0.0),
        WorldVector::new(0.0, 0.0, 1.0),
        ScreenSize::new(2048, 1536),
        36e-3,
        50e-3,
        4.8,
        5.0,
    );
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
                let mut render_progress = render(scene, camera, settings, |_| {}, |_| {}).unwrap();
                render_progress.wait();
            },
            criterion::BatchSize::LargeInput,
        )
    });
}

criterion_group!(benches, criterion_benchmark);
criterion_main!(benches);
