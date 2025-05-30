use std::{
    ops::Deref,
    sync::{Arc, Mutex},
};

use eframe::{App, CreationContext, Frame, egui};
use egui::{CentralPanel, Color32, ColorImage, Image, TextureOptions};
use image::{GenericImageView, Rgba};
use minipath::{
    Camera, RenderProgress, RenderSettings, Scene,
    geometry::{ScreenBlock, ScreenSize, WorldDistance, WorldPoint, WorldVector},
    render,
};

pub struct MinipathGui {
    render_progress: RenderProgress,
    texture: egui::TextureHandle,
    started: Arc<Mutex<Vec<ScreenBlock>>>,
    dirty: Arc<Mutex<Vec<ScreenBlock>>>,
}

impl MinipathGui {
    pub fn new(
        scene: Scene,
        camera: Camera,
        render_settings: RenderSettings,
        cc: &CreationContext<'_>,
    ) -> anyhow::Result<Self> {
        let started = Arc::new(Mutex::new(Vec::new()));
        let tile_started_callback = {
            let started = Arc::clone(&started);
            let ctx = cc.egui_ctx.clone();
            move |tile| {
                started.lock().unwrap().push(tile);
                ctx.request_repaint();
            }
        };
        let dirty = Arc::new(Mutex::new(Vec::new()));
        let tile_finished_callback = {
            let dirty = Arc::clone(&dirty);
            let ctx = cc.egui_ctx.clone();
            move |tile| {
                dirty.lock().unwrap().push(tile);
                ctx.request_repaint();
            }
        };
        let render_progress = render(
            scene,
            camera,
            render_settings,
            tile_started_callback,
            tile_finished_callback,
        )?;
        let texture = cc.egui_ctx.load_texture(
            "rendered",
            create_image(render_progress.image().lock().unwrap().deref()),
            TextureOptions::LINEAR,
        );

        Ok(MinipathGui {
            render_progress,
            texture,
            started,
            dirty,
        })
    }
}

impl App for MinipathGui {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut Frame) {
        for tile in self.started.lock().unwrap().drain(..) {
            self.texture.set_partial(
                [tile.min.x as usize, tile.min.y as usize],
                create_in_progress_tile(tile.width(), tile.height()),
                TextureOptions::LINEAR,
            );
        }

        {
            let mut dirty = self.dirty.lock().unwrap();

            if !dirty.is_empty() {
                let img = self.render_progress.image().lock().unwrap();

                for tile in dirty.drain(..) {
                    let tile_img = img.view(tile.min.x, tile.min.y, tile.width(), tile.height());
                    let color_image = create_image(tile_img.deref());

                    self.texture.set_partial(
                        [tile.min.x as usize, tile.min.y as usize],
                        color_image,
                        TextureOptions::LINEAR,
                    );
                }
            }
        }

        CentralPanel::default().show(ctx, |ui| {
            ui.centered_and_justified(|ui| {
                ui.add(Image::from_texture(&self.texture).shrink_to_fit())
            })
        });
    }
}

fn main() -> anyhow::Result<()> {
    eframe::run_native(
        "Minipath GUI",
        Default::default(),
        Box::new(|cc| {
            let camera = Camera::new(
                WorldPoint::new(0.0, 0.0, 2.0),
                WorldVector::new(0.0, 1.0, 0.0),
                WorldVector::new(0.0, 0.0, 1.0),
                ScreenSize::new(2048, 1536),
                WorldDistance::new(36e-3),
                WorldDistance::new(50e-3),
                4.8,
                WorldDistance::new(5.0),
            );
            let settings = RenderSettings {
                tile_size: 64.try_into().unwrap(),
                sample_count: 100.try_into().unwrap(),
            };

            Ok(Box::new(MinipathGui::new(
                Scene::default(),
                camera,
                settings,
                cc,
            )?))
        }),
    )
    .unwrap();

    Ok(())
}

fn create_image(img: &impl GenericImageView<Pixel = Rgba<u8>>) -> ColorImage {
    let mut pixels = Vec::with_capacity(img.width() as usize * img.height() as usize);
    pixels.extend(
        img.pixels().map(|(_x, _y, px)| {
            Color32::from_rgba_unmultiplied(px.0[0], px.0[1], px.0[2], px.0[3])
        }),
    );
    ColorImage {
        size: [img.width() as usize, img.height() as usize],
        pixels,
    }
}

fn create_in_progress_tile(width: u32, height: u32) -> ColorImage {
    let width = width as usize;
    let height = height as usize;
    let mut pixels = Vec::with_capacity(width * height);

    for y in 0..width {
        for x in 0..height {
            let bw = 3;
            let border = (x < bw) | (y < bw) | (x >= (width - bw)) | (y > (height - bw));
            if border {
                pixels.push(Color32::from_rgba_unmultiplied(200, 100, 100, 255));
            } else {
                pixels.push(Color32::from_rgba_unmultiplied(0, 0, 0, 0));
            }
        }
    }

    ColorImage {
        size: [width, height],
        pixels,
    }
}
