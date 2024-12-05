use crate::geometry::*;
use crate::image_buffer;

use image;
use parking_lot;
use sdl2;

use image::GenericImage;
use image::GenericImageView;

const SDL_PIXEL_FORMAT: sdl2::pixels::PixelFormatEnum = sdl2::pixels::PixelFormatEnum::ABGR8888;
type PixelType = image::Rgba<u8>;

pub struct ImageWindow {
    title: String,
    size: ScreenSize,

    context: sdl2::Sdl,
    event: sdl2::EventSubsystem,

    img: parking_lot::Mutex<image::RgbaImage>,
}

impl ImageWindow {
    /// Creates a SDL window.
    /// There can be only one!
    pub fn new(title: &str, width: u32, height: u32) -> anyhow::Result<ImageWindow> {
        let context = sdl2::init().map_err(anyhow::Error::msg)?;
        let event = context.event().map_err(anyhow::Error::msg)?;

        event
            .register_custom_event::<ScreenBlock>()
            .map_err(anyhow::Error::msg)?;

        Ok(ImageWindow {
            title: String::from(title),
            size: ScreenSize::new(width, height),

            context,
            event,

            img: parking_lot::Mutex::new(image::ImageBuffer::<PixelType, _>::new(width, height)),
        })
    }
}

impl image_buffer::ImageBuffer for ImageWindow {
    /// Runs SDL event loop and handles the window.
    /// Only exits when the window is closed.
    fn run(&self) -> anyhow::Result<()> {
        let video = self.context.video().map_err(anyhow::Error::msg)?;
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

        update_texture(&self.img.lock(), &mut texture, self.size.into())?; // Copy the empty output to texture

        let mut events = self.context.event_pump().map_err(anyhow::Error::msg)?;

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
                    if let Some(rendered) = event.as_user_event_type::<ScreenBlock>() {
                        update_texture(&self.img.lock(), &mut texture, rendered)?;
                        redraw(&mut canvas, &texture)?;
                    }
                }
            }
        }
        Ok(())
    }

    /// Creates a writer function that can write data into the window from different thread.
    fn make_writer<'a>(&'a self) -> Box<dyn image_buffer::ImageBufferWriter + 'a> {
        Box::new(Writer {
            event_sender: self.event.event_sender(),
            img: &self.img,
        })
    }

    fn save(&self, path: &std::path::Path) -> anyhow::Result<()> {
        self.img.lock().save(path)?;
        Ok(())
    }
}

pub struct Writer<'a> {
    event_sender: sdl2::event::EventSender,
    img: &'a parking_lot::Mutex<image::RgbaImage>,
}

impl<'a> image_buffer::ImageBufferWriter for Writer<'a> {
    fn write(&self, block: ScreenBlock, block_buffer: &image::RgbaImage) -> anyhow::Result<()> {
        debug_assert!(block.width() <= block_buffer.width());
        debug_assert!(block.height() <= block_buffer.height());

        self.img
            .lock()
            .copy_from(block_buffer, block.min.x, block.min.y)?;
        self.event_sender
            .push_custom_event(block)
            .map_err(anyhow::Error::msg)?;

        Ok(())
    }
}

/// Copies a block from the image to the texture (to the gpu).
fn update_texture(
    img: &image::RgbaImage,
    texture: &mut sdl2::render::Texture,
    block: ScreenBlock,
) -> anyhow::Result<()> {
    let rect = sdl2::rect::Rect::new(
        block.min.x as i32,
        block.min.y as i32,
        block.width(),
        block.height(),
    );

    texture
        .with_lock(
            Some(rect),
            |texture_buffer: &mut [u8], pitch: usize| -> anyhow::Result<()> {
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
                    &img.view(block.min.x, block.min.y, block.width(), block.height()),
                    0,
                    0,
                )?;
                Ok(())
            },
        )
        .map_err(anyhow::Error::msg)??;

    Ok(())
}

/// Completely redraws the canvas, puts a checkerboard behind and draws the texture on top.
fn redraw(
    canvas: &mut sdl2::render::Canvas<sdl2::video::Window>,
    texture: &sdl2::render::Texture,
) -> anyhow::Result<()> {
    draw_checkerboard(canvas)?;
    canvas
        .copy(texture, None, None)
        .map_err(anyhow::Error::msg)?;
    canvas.present();

    Ok(())
}

/// Clears the canvas with a checkerboard pattern.
fn draw_checkerboard(canvas: &mut sdl2::render::Canvas<sdl2::video::Window>) -> anyhow::Result<()> {
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
            canvas.fill_rect(Some(rect)).map_err(anyhow::Error::msg)?;
        }
    }

    Ok(())
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    #[ignore]
    fn test_image_window() {
        const WIDTH: u32 = 200;
        const HEIGHT: u32 = 200;
        const CHUNK_SIZE: u32 = 51;

        let mut window = ImageWindow::new("ImageWindow test", WIDTH, HEIGHT).unwrap();
        image_buffer::test::test_image_buffer(WIDTH, HEIGHT, CHUNK_SIZE, &mut window);
    }
}
