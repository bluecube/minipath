use std::sync::Arc;

use minipath::{
    Camera, RenderSettings, Scene,
    geometry::{ScreenSize, WorldPoint, WorldVector},
    render,
    scene::triangle_bvh::TriangleBvh,
};

use indicatif::ProgressBar;

fn main() -> anyhow::Result<()> {
    let camera = Camera::default()
        .look_at(
            WorldPoint::new(0.0, 2.0, 10.0),
            WorldPoint::new(0.0, 1.5, 0.0),
            WorldVector::new(0.0, 1.0, 0.0),
        )
        .f_number(4.8)
        .focus_distance(10.0);

    let settings = RenderSettings {
        tile_size: 64.try_into().unwrap(),
        sample_count: 100.try_into().unwrap(),
        resolution: ScreenSize::new(2048, 1536),
    };
    let scene = Arc::new(Scene {
        object: TriangleBvh::with_obj("data/teapot.obj").unwrap(),
    });
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
