use std::{
    ops::Deref as _,
    sync::{
        Arc, Mutex,
        atomic::{AtomicUsize, Ordering},
    },
    thread::{self, JoinHandle},
};

use image::{GenericImage, GenericImageView, RgbaImage};

use crate::{
    camera::Camera,
    geometry::ScreenBlock,
    renderer::{RenderSettings, worker::Worker},
    scene::{Object, Scene},
    screen_block::ScreenBlockExt,
};

pub fn render<
    O: Object + Send + Sync + 'static,
    F1: Fn(ScreenBlock) + Send + Sync + 'static,
    F2: Fn(ScreenBlock) + Send + Sync + 'static,
>(
    scene: Scene<O>,
    camera: Camera,
    settings: RenderSettings,
    started_tile_callback: F1,
    finished_tile_callback: F2,
) -> anyhow::Result<RenderProgress<O>> {
    let image = RgbaImage::new(
        camera.get_resolution().width,
        camera.get_resolution().height,
    );
    let state = Arc::new(RenderState {
        scene,
        camera,
        settings,

        image: Mutex::new(image),

        tile_ordering: ScreenBlock::from_size(camera.get_resolution())
            .tile_ordering(settings.tile_size),
        next_tile_index: AtomicUsize::new(0),
    });
    let started_tile_callback = Arc::new(started_tile_callback);
    let finished_tile_callback = Arc::new(finished_tile_callback);

    let cores = core_affinity::get_core_ids()
        .expect("We need a CPU list!")
        .into_iter()
        .enumerate();

    let threads = cores
        .map(|(worker_id, core)| {
            let state = Arc::clone(&state);
            let started_tile_callback = Arc::clone(&started_tile_callback);
            let finished_tile_callback = Arc::clone(&finished_tile_callback);

            thread::Builder::new()
                .name(format!("worker{worker_id}"))
                .spawn(move || {
                    core_affinity::set_for_current(core);

                    let mut worker = Worker::<O>::new(worker_id);
                    let mut buffer =
                        RgbaImage::new(settings.tile_size.into(), settings.tile_size.into());

                    while let Some(tile) = state.get_next_tile() {
                        (started_tile_callback)(tile.clone());

                        worker.render_tile(
                            &state.scene,
                            &state.camera,
                            &state.settings,
                            tile,
                            &mut buffer,
                        );
                        state
                            .image
                            .lock()
                            .expect("Poisoned lock!")
                            .copy_from(
                                buffer.view(0, 0, tile.width(), tile.height()).deref(),
                                tile.min.x,
                                tile.min.y,
                            )
                            .unwrap_or_else(|_| {
                                unreachable!("The buffer should always fit into the output")
                            });

                        (finished_tile_callback)(tile.clone());
                    }
                })
        })
        .collect::<Result<Vec<_>, _>>()?;

    Ok(RenderProgress {
        render_state: state,
        threads,
    })
}

pub struct RenderProgress<O: Object> {
    render_state: Arc<RenderState<O>>,
    threads: Vec<JoinHandle<()>>,
}

impl<O: Object> RenderProgress<O> {
    /// Return number of processed and total tiles.
    pub fn progress(&self) -> (usize, usize) {
        let total = self.render_state.tile_ordering.len();
        let processed = self
            .render_state
            .next_tile_index
            .load(Ordering::Acquire)
            .min(total);
        (processed, total)
    }

    pub fn progress_percent(&self) -> f32 {
        let (processed, total) = self.progress();
        100.0 * (processed as f32) / (total as f32)
    }

    pub fn is_finished(&self) -> bool {
        self.threads.iter().all(|handle| handle.is_finished())
    }

    /// Signal the workers to abort.
    /// Any running workers will still finish their tiles, but no new ones will be started.
    pub fn abort(&self) {
        self.render_state
            .next_tile_index
            .store(self.render_state.tile_ordering.len(), Ordering::Release);
    }

    /// Wait for the workers to finish.
    /// Does not block
    pub fn wait(&mut self) {
        self.threads
            .drain(..)
            .for_each(|handle| handle.join().unwrap());
    }

    pub fn image(&self) -> &Mutex<RgbaImage> {
        &self.render_state.image
    }
}

struct RenderState<O: Object> {
    scene: Scene<O>,
    camera: Camera,
    settings: RenderSettings,

    image: Mutex<RgbaImage>,

    tile_ordering: Vec<ScreenBlock>,
    next_tile_index: AtomicUsize,
}

impl<O: Object> RenderState<O> {
    fn get_next_tile(&self) -> Option<&ScreenBlock> {
        let id = self.next_tile_index.fetch_add(1, Ordering::AcqRel);
        self.tile_ordering.get(id)
    }
}
