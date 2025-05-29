use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};

use eframe::{App, CreationContext, egui};
use egui::ColorImage;
use image::RgbaImage;
use minipath::{
    Camera, RenderProgress, RenderSettings, Scene,
    geometry::{ScreenSize, WorldDistance, WorldPoint, WorldVector},
    render,
};

pub struct MinipathGui {
    render_progress: RenderProgress,
    texture: egui::TextureHandle,
    dirty: Arc<AtomicBool>,
}

impl MinipathGui {
    pub fn new(
        scene: Scene,
        camera: Camera,
        render_settings: RenderSettings,
        cc: &CreationContext<'_>,
    ) -> anyhow::Result<Self> {
        let dirty_flag = Arc::new(AtomicBool::new(false));

        let callback = {
            let dirty_flag = Arc::clone(&dirty_flag);
            let ctx = cc.egui_ctx.clone();
            move || {
                dirty_flag.store(true, Ordering::Release);
                ctx.request_repaint();
            }
        };
        let render_progress = render(scene, camera, render_settings, callback)?;
        let texture = cc.egui_ctx.load_texture(
            "rendered",
            create_image(&render_progress.image().lock().unwrap()),
            egui::TextureOptions::LINEAR,
        );

        Ok(MinipathGui {
            render_progress,
            texture,
            dirty: dirty_flag,
        })
    }
}

impl App for MinipathGui {
    fn update(&mut self, ctx: &egui::Context, frame: &mut eframe::Frame) {
        if self.dirty.swap(false, Ordering::AcqRel) {
            self.texture.set(
                create_image(&self.render_progress.image().lock().unwrap()),
                egui::TextureOptions::LINEAR,
            );
        }

        egui::CentralPanel::default().show(ctx, |ui| {
            ui.centered_and_justified(|ui| {
                ui.add(egui::Image::from_texture(&self.texture).shrink_to_fit())
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

fn create_image(img: &RgbaImage) -> ColorImage {
    ColorImage::from_rgba_unmultiplied(
        [img.width() as usize, img.height() as usize],
        img.as_flat_samples().as_slice(),
    )
}
