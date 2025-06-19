use minipath::{
    Camera, RenderSettings, Scene,
    geometry::{ScreenSize, WorldPoint, WorldVector},
    render,
    scene::triangle_bvh::TriangleBvh,
};

use indicatif::ProgressBar;

fn main() -> anyhow::Result<()> {
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
        sample_count: 100.try_into().unwrap(),
    };
    let scene = Scene {
        object: TriangleBvh::with_obj("data/teapot.obj").unwrap(),
    };
    scene.object.print_statistics();

    let bar = ProgressBar::no_length();
    let mut render_progress = render(scene, camera, settings, |_| {}, {
        let bar = bar.clone();
        move |_, progress| {
            bar.update(|ps| {
                ps.set_len(progress.total as u64);
                ps.set_pos(progress.finished as u64)
            })
        }
    })?;
    bar.set_length(render_progress.progress().total as u64);

    render_progress.wait();

    Ok(())
}
