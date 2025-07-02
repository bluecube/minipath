use std::{
    ops::Deref,
    sync::{Arc, Mutex},
};

use assert2::assert;
use eframe::{App, CreationContext, Frame, egui};
use egui::{CentralPanel, Color32, ColorImage, Image, TextureOptions};
use image::{GenericImageView, Rgba};
use minipath::{
    Camera, RenderProgress, RenderSettings, Scene,
    geometry::{ScreenBlock, ScreenPoint, ScreenSize, WorldPoint, WorldVector},
    render,
    scene::{Object, triangle_bvh::TriangleBvh},
};
use nalgebra::{Translation3, Vector2};

pub struct MinipathGui<O: Object> {
    render_progress: RenderProgress<O>,

    pending_tiles: Arc<Mutex<Vec<(ScreenBlock, bool)>>>,
    texture: egui::TextureHandle,

    scene: Arc<Scene<O>>,
    camera: Camera,
    full_render_settings: RenderSettings,
    preview_render_settings: RenderSettings,

    draw_state: DrawState,
}

impl<O: Object + Send + Sync + 'static> MinipathGui<O> {
    pub fn new(
        scene: Arc<Scene<O>>,
        camera: Camera,
        preview_render_settings: RenderSettings,
        full_render_settings: RenderSettings,
        cc: &CreationContext<'_>,
    ) -> anyhow::Result<Self> {
        assert!(preview_render_settings.resolution == full_render_settings.resolution);

        let pending_tiles = Arc::new(Mutex::new(Vec::new()));
        let render_progress = Self::start_render(
            Arc::clone(&pending_tiles),
            Arc::clone(&scene),
            camera.clone(),
            preview_render_settings.clone(),
            cc.egui_ctx.clone(),
        )?;
        let screen_block =
            ScreenBlock::with_size(ScreenPoint::origin(), &full_render_settings.resolution);
        let texture = cc.egui_ctx.load_texture(
            "rendered",
            egui_image(
                &screen_block,
                render_progress.image().lock().unwrap().deref(),
                false,
            ),
            TextureOptions::LINEAR,
        );

        Ok(MinipathGui {
            render_progress,
            pending_tiles,
            texture,
            scene,
            camera,
            full_render_settings,
            preview_render_settings,
            draw_state: DrawState::FullRender,
        })
    }

    fn start_render(
        pending_tiles: Arc<Mutex<Vec<(ScreenBlock, bool)>>>,
        scene: Arc<Scene<O>>,
        camera: Camera,
        render_settings: RenderSettings,
        ctx: egui::Context,
    ) -> anyhow::Result<RenderProgress<O>> {
        pending_tiles.lock().unwrap().clear();

        let started_tile_callback = {
            let pending_tiles = Arc::clone(&pending_tiles);
            let ctx = ctx.clone();
            move |tile| {
                pending_tiles.lock().unwrap().push((tile, true));
                ctx.request_repaint();
            }
        };

        let finished_tile_callback = move |tile, _| {
            pending_tiles.lock().unwrap().push((tile, false));
            ctx.request_repaint();
        };

        render(
            scene,
            camera.clone(),
            render_settings,
            started_tile_callback,
            finished_tile_callback,
        )
    }

    fn cancel_previous_render(&mut self) {
        self.reload_texture();
        self.render_progress.abort();
    }

    fn reload_texture(&mut self) {
        let screen_block =
            ScreenBlock::with_size(ScreenPoint::origin(), &self.full_render_settings.resolution);
        self.texture.set(
            egui_image(
                &screen_block,
                self.render_progress.image().lock().unwrap().deref(),
                false,
            ),
            TextureOptions::LINEAR,
        );
    }

    fn start_preview_render(&mut self, ctx: egui::Context) {
        self.cancel_previous_render();
        self.draw_state = DrawState::Preview;
        self.render_progress = Self::start_render(
            Arc::clone(&self.pending_tiles),
            Arc::clone(&self.scene),
            self.camera.clone(),
            self.preview_render_settings.clone(),
            ctx,
        )
        .unwrap();
    }

    fn start_full_render(&mut self, ctx: egui::Context) {
        self.cancel_previous_render();
        self.draw_state = DrawState::FullRender;
        self.render_progress = Self::start_render(
            Arc::clone(&self.pending_tiles),
            Arc::clone(&self.scene),
            self.camera.clone(),
            self.full_render_settings.clone(),
            ctx,
        )
        .unwrap();
    }
}

impl<O: Object + Send + Sync + 'static> App for MinipathGui<O> {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut Frame) {
        {
            let mut pending_tiles = self.pending_tiles.lock().unwrap();
            if !pending_tiles.is_empty() {
                let img = self.render_progress.image().lock().unwrap();

                for (tile, in_progress) in pending_tiles.drain(..) {
                    let tile_img = img.view(tile.min.x, tile.min.y, tile.width(), tile.height());
                    let color_image = egui_image(&tile, tile_img.deref(), in_progress);

                    self.texture.set_partial(
                        [tile.min.x as usize, tile.min.y as usize],
                        color_image,
                        TextureOptions::LINEAR,
                    );
                }
            }
        }

        if self.render_progress.is_finished() && self.draw_state == DrawState::Preview {
            self.start_full_render(ctx.clone());
        }

        CentralPanel::default().show(ctx, |ui| {
            ui.centered_and_justified(|ui| {
                ui.add(Image::from_texture(&self.texture).shrink_to_fit())
            })
        });

        ctx.input(|i| {
            let translation_speed = 2.0 * i.stable_dt;

            let x_speed = (i.key_pressed(egui::Key::ArrowRight) as i32)
                - (i.key_pressed(egui::Key::ArrowLeft) as i32);
            let z_speed = (i.key_pressed(egui::Key::ArrowDown) as i32)
                - (i.key_pressed(egui::Key::ArrowUp) as i32);

            if x_speed != 0 || z_speed != 0 {
                let x_speed = x_speed as f32 * translation_speed;
                let z_speed = z_speed as f32 * translation_speed;
                let translation = Translation3::new(x_speed, 0.0, z_speed);

                self.camera = self.camera.transformed(translation.into());

                self.start_preview_render(ctx.clone());
            }
        });
    }
}

fn main() -> anyhow::Result<()> {
    eframe::run_native(
        "Minipath GUI",
        Default::default(),
        Box::new(|cc| {
            let camera = Camera::default()
                .look_at(
                    WorldPoint::new(0.0, 2.0, 10.0),
                    WorldPoint::new(0.0, 1.5, 0.0),
                    WorldVector::new(0.0, 1.0, 0.0),
                )
                .f_number(4.8)
                .focus_distance(10.0);

            let full_settings = RenderSettings {
                tile_size: 64.try_into().unwrap(),
                sample_count: 2.try_into().unwrap(),
                resolution: ScreenSize::new(2048, 1536),
            };
            let preview_settings = RenderSettings {
                sample_count: 1.try_into().unwrap(),
                ..full_settings
            };
            let scene = Arc::new(Scene {
                object: TriangleBvh::with_obj("data/teapot.obj").unwrap(),
            });
            scene.object.print_statistics();

            Ok(Box::new(MinipathGui::new(
                scene,
                camera,
                preview_settings,
                full_settings,
                cc,
            )?))
        }),
    )
    .unwrap();

    Ok(())
}

fn egui_image(
    tile: &ScreenBlock,
    img: &impl GenericImageView<Pixel = Rgba<u8>>,
    in_progress: bool,
) -> ColorImage {
    let width = img.width();
    let height = img.height();
    let mut pixels = Vec::with_capacity((width * height) as usize);
    pixels.extend(img.pixels().map(|(x, y, px)| {
        let bw = 4u32;
        let border = (x < bw) | (y < bw) | (x >= (width - bw)) | (y > (height - bw));
        if in_progress && border {
            Color32::from_rgba_unmultiplied(200, 100, 100, 255)
        } else {
            let p = tile.min + Vector2::new(x, y);
            let grid_color = background_grid(p);
            let image_color = Color32::from_rgba_unmultiplied(px.0[0], px.0[1], px.0[2], px.0[3]);
            grid_color.blend(image_color)
        }
    }));
    ColorImage {
        size: [width as usize, height as usize],
        pixels,
    }
}

fn background_grid(p: ScreenPoint) -> Color32 {
    let grid_size = 16;
    let square_x = p.x / grid_size;
    let square_y = p.y / grid_size;

    let dark = (square_x + square_y) & 1 == 0;

    if dark {
        Color32::from_rgb(50, 50, 50)
    } else {
        Color32::from_rgb(93, 93, 93)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum DrawState {
    Preview,
    FullRender,
}
