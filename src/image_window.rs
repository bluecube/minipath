use crate::screen_block;

use image;
use sdl2;
use std::sync;

use image::GenericImage;
use image::GenericImageView;

const SDL_PIXEL_FORMAT: sdl2::pixels::PixelFormatEnum = sdl2::pixels::PixelFormatEnum::ABGR8888;
type PixelType = image::Rgba<u8>;

type AnyError = Box<dyn std::error::Error>;
type SimpleResult = Result<(), AnyError>;

pub struct ImageWindow {
    title: String,
    size: screen_block::ScreenSize,

    context: sdl2::Sdl,
    event: sdl2::EventSubsystem,

    img: sync::Mutex<image::RgbaImage>,
}

impl ImageWindow {
    /// Creates a SDL window.
    /// There can be only one!
    pub fn new(title: &str, width: u32, height: u32) -> Result<ImageWindow, AnyError> {
        let context = sdl2::init()?;
        let event = context.event()?;

        event
            .register_custom_event::<screen_block::ScreenBlock>()?;

        Ok(ImageWindow {
            title: String::from(title),
            size: screen_block::ScreenSize::new(width, height),

            context,
            event,

            img: sync::Mutex::new(image::ImageBuffer::<PixelType, _>::new(width, height)),
        })
    }

    /// Runs SDL event loop and handles the window.
    /// Only exits when the window is closed.
    pub fn run(&self) -> SimpleResult {
        let video = self.context.video()?;
        let mut canvas = video
            .window(&self.title, self.size.width, self.size.height)
            .position_centered()
            .resizable()
            .build()?
            .into_canvas()
            .build()?;
        canvas.set_logical_size(self.size.width, self.size.height)?;

        let texture_creator = canvas.texture_creator();
        let mut texture = texture_creator.create_texture_streaming(
            SDL_PIXEL_FORMAT,
            self.size.width,
            self.size.height,
        )?;
        texture.set_blend_mode(sdl2::render::BlendMode::Blend);

        self.update_texture(&mut texture, self.size.into())?; // Copy the empty output to texture

        let mut events = self.context.event_pump()?;

        for event in events.wait_iter() {
            use sdl2::event::Event;
            use sdl2::event::WindowEvent;
            use sdl2::keyboard::Keycode;
            match event {
                Event::Quit { .. }
                | Event::KeyDown {
                    keycode: Some(Keycode::Escape),
                    ..
                }
                | Event::KeyDown {
                    keycode: Some(Keycode::Q),
                    ..
                } => break,

                Event::Window {
                    win_event: WindowEvent::Exposed,
                    ..
                } => redraw(&mut canvas, &texture)?,

                _ => {
                    if let Some(rendered) = event.as_user_event_type::<screen_block::ScreenBlock>()
                    {
                        self.update_texture(&mut texture, rendered)?;
                        redraw(&mut canvas, &texture)?;
                    }
                }
            }
        }
        Ok(())
    }

    /// Creates a writer function that can write data into the window from different thread.
    pub fn make_writer(
        &self,
    ) -> impl Fn(screen_block::ScreenBlock, image::RgbaImage) -> SimpleResult + '_ {
        let event_sender = self.event.event_sender();
        let img = &self.img;
        move |block, block_buffer| {
            debug_assert_eq!(block_buffer.width(), block.width());
            debug_assert_eq!(block_buffer.height(), block.width());

            (*img.lock().unwrap()).copy_from(&block_buffer, block.min.x, block.min.y)?;
            event_sender
                .push_custom_event(block)?;

            Ok(())
        }
    }

    /// Copies a block from the image to the texture (to the gpu).
    fn update_texture(
        &self,
        texture: &mut sdl2::render::Texture,
        block: screen_block::ScreenBlock,
    ) -> SimpleResult {
        let img = self.img.lock().unwrap();

        let rect = sdl2::rect::Rect::new(
            block.min.x as i32,
            block.min.y as i32,
            block.width(),
            block.height(),
        );

        texture
            .with_lock(
                Some(rect),
                |texture_buffer: &mut [u8], pitch: usize| -> SimpleResult {
                    // Obtain view to the part of the texture that we are updating.
                    let mut texture_samples = image::flat::FlatSamples {
                        samples: texture_buffer,
                        layout: image::flat::SampleLayout {
                            channels: 4,       // There is no place to get this value programatically
                            channel_stride: 1, // There is no place to get this value programatically
                            width: block.width(),
                            width_stride: SDL_PIXEL_FORMAT.byte_size_per_pixel(),
                            height: block.height(),
                            height_stride: pitch,
                        },
                        color_hint: None,
                    };
                    let mut texture_view = texture_samples.as_view_mut::<PixelType>().unwrap();
                    texture_view.copy_from(
                        &(*img).view(block.min.x, block.min.y, block.width(), block.height()),
                        0,
                        0,
                    )?;
                    Ok(())
                },
            )??;

        Ok(())
    }
}

/// Completely redraws the canvas, puts a checkerboard behind and draws the texture on top.
fn redraw(
    canvas: &mut sdl2::render::Canvas<sdl2::video::Window>,
    texture: &sdl2::render::Texture,
) -> SimpleResult {
    draw_checkerboard(canvas)?;
    canvas.copy(texture, None, None)?;
    canvas.present();

    Ok(())
}

/// Clears the canvas with a checkerboard pattern.
fn draw_checkerboard(canvas: &mut sdl2::render::Canvas<sdl2::video::Window>) -> SimpleResult {
    canvas.set_draw_color(sdl2::pixels::Color::RGB(50, 50, 50));
    canvas.clear();
    canvas.set_draw_color(sdl2::pixels::Color::RGB(200, 200, 200));

    let (w, h) = canvas.logical_size();
    let checkerboard_size = 20;

    for y in 0..(h / checkerboard_size) {
        for x in ((y % 2)..(w / checkerboard_size)).step_by(2) {
            let rect = sdl2::rect::Rect::new(
                (x * checkerboard_size) as i32,
                (y * checkerboard_size) as i32,
                checkerboard_size,
                checkerboard_size,
            );
            canvas.fill_rect(Some(rect))?;
        }
    }

    Ok(())
}
