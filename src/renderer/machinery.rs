use std::{
    ops::Deref as _,
    sync::{
        Arc, Mutex,
        atomic::{AtomicUsize, Ordering},
    },
    thread::{self, JoinHandle},
    time::{Duration, Instant},
};

use image::{GenericImage, GenericImageView, RgbaImage};

use crate::{
    camera::Camera,
    geometry::{ScreenBlock, ScreenPoint},
    renderer::{RenderSettings, worker::Worker},
    scene::{Object, Scene},
};

pub fn render<
    O: Object + Send + Sync + 'static,
    F1: Fn(ScreenBlock) + Send + Sync + 'static,
    F2: Fn(ScreenBlock, RenderProgressSnapshot) + Send + Sync + 'static,
>(
    scene: Scene<O>,
    camera: Camera,
    settings: RenderSettings,
    started_tile_callback: F1,
    finished_tile_callback: F2,
) -> anyhow::Result<RenderProgress<O>> {
    let cores = core_affinity::get_core_ids().expect("We need a CPU list!");
    let worker_count = cores.len();

    let image = RgbaImage::new(camera.get_resolution().x, camera.get_resolution().y);
    let state = Arc::new(RenderState {
        scene,
        camera,
        settings,

        image: Mutex::new(image),

        tile_ordering: ScreenBlock::with_size(ScreenPoint::origin(), &camera.get_resolution())
            .tile_ordering(settings.tile_size),
        next_tile_index: AtomicUsize::new(0),

        start_time: Instant::now(),
        end: Mutex::new((0, None)),
    });
    let started_tile_callback = Arc::new(started_tile_callback);
    let finished_tile_callback = Arc::new(finished_tile_callback);

    let threads = cores
        .into_iter()
        .enumerate()
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
                    let tile_count = state.tile_ordering.len();

                    let (_, Some(mut tile)) = state.get_next_tile() else {
                        return;
                    };

                    loop {
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

                        let (new_tile_id, new_tile) = state.get_next_tile();

                        (finished_tile_callback)(
                            tile.clone(),
                            RenderProgressSnapshot {
                                finished: new_tile_id.saturating_sub(worker_count),
                                total: tile_count,
                            },
                        );

                        match new_tile {
                            Some(new_tile) => tile = new_tile,
                            None => break,
                        }
                    }

                    let elapsed = Instant::elapsed(&state.start_time);
                    let mut lock = state.end.lock().unwrap();

                    lock.0 += 1;
                    if lock.0 == worker_count {
                        lock.1 = Some(elapsed);
                    }
                })
        })
        .collect::<Result<Vec<_>, _>>()?;

    Ok(RenderProgress {
        render_state: state,
        worker_count,
        threads,
    })
}

pub struct RenderProgress<O: Object> {
    render_state: Arc<RenderState<O>>,
    worker_count: usize,
    threads: Vec<JoinHandle<()>>,
}

impl<O: Object> RenderProgress<O> {
    /// Return number of processed and total tiles.
    pub fn progress(&self) -> RenderProgressSnapshot {
        RenderProgressSnapshot {
            finished: self
                .render_state
                .next_tile_index
                .load(Ordering::Acquire)
                .saturating_sub(self.worker_count),
            total: self.render_state.tile_ordering.len(),
        }
    }

    pub fn is_finished(&self) -> bool {
        self.threads.iter().all(|handle| handle.is_finished())
    }

    /// Returns elapsed time since the start of the render. Stops
    /// incrementing once the render finishes.
    pub fn elapsed(&self) -> Duration {
        self.render_state
            .end
            .lock()
            .unwrap()
            .1
            .unwrap_or_else(|| self.render_state.start_time.elapsed())
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

pub struct RenderProgressSnapshot {
    pub finished: usize,
    pub total: usize,
}

impl RenderProgressSnapshot {
    pub fn percent(&self) -> f32 {
        100.0 * (self.finished as f32) / (self.total as f32)
    }
}

struct RenderState<O: Object> {
    scene: Scene<O>,
    camera: Camera,
    settings: RenderSettings,

    image: Mutex<RgbaImage>,

    tile_ordering: Vec<ScreenBlock>,
    next_tile_index: AtomicUsize,

    start_time: Instant,
    /// Number of workers that finished, elapsed time
    end: Mutex<(usize, Option<Duration>)>,
}

impl<O: Object> RenderState<O> {
    fn get_next_tile(&self) -> (usize, Option<&ScreenBlock>) {
        let id = self.next_tile_index.fetch_add(1, Ordering::AcqRel);
        (id, self.tile_ordering.get(id))
    }
}
