use sdl2;
use anyhow;

pub struct ImageWindow {
    context: sdl2::Sdl,
    canvas: sdl2::render::Canvas<sdl2::video::Window>
}

impl ImageWindow {
    /// Creates a SDL window.
    /// There can be only one!
    pub fn new(title: &str, width: u32, height: u32) -> anyhow::Result<ImageWindow> {
        let context = sdl2::init().map_err(anyhow_from_string)?;
        let video = context.video().map_err(anyhow_from_string)?;
        let mut canvas = video.window(title, width, height)
            .position_centered()
            .resizable()
            .build()?
            .into_canvas()
            .build()?;
        canvas.set_logical_size(width, height)?;

        let mut window = ImageWindow {
            context: context,
            canvas: canvas,
        };

        window.draw_checkerboard()?;


        Ok(window)
    }

    /// Runs SDL event loop and handles the window.
    /// Only exits when the window is closed.
    pub fn event_loop(&mut self) -> anyhow::Result<()> {
        let mut events = self.context.event_pump().map_err(anyhow_from_string)?;

        for event in events.wait_iter() {
            use sdl2::event::Event;
            use sdl2::event::WindowEvent;
            use sdl2::keyboard::Keycode;
            match event {
                Event::Quit {..}
                    | Event::KeyDown {keycode: Some(Keycode::Escape), ..}
                    | Event::KeyDown {keycode: Some(Keycode::Q), ..} => break,

                Event::Window {win_event: WindowEvent::Exposed, ..} => self.redraw()?,

                _ => {},
            }
        }
        Ok(())
    }

    fn redraw(&mut self) -> anyhow::Result<()> {
        self.draw_checkerboard()
    }

    fn draw_checkerboard(&mut self) -> anyhow::Result<()> {
        self.canvas.set_draw_color(sdl2::pixels::Color::RGB(50, 50, 50));
        self.canvas.clear();
        self.canvas.set_draw_color(sdl2::pixels::Color::RGB(200, 200, 200));

        let (w, h) = self.canvas.logical_size();
        let checkerboard_size = 20;

        for y in 0..(h / checkerboard_size) {
            for x in ((y % 2)..(w / checkerboard_size)).step_by(2) {
                let rect = sdl2::rect::Rect::new((x * checkerboard_size) as i32,
                                                 (y * checkerboard_size) as i32,
                                                 checkerboard_size,
                                                 checkerboard_size);
                self.canvas.fill_rect(Some(rect)).map_err(anyhow_from_string)?;
            }
        }

        self.canvas.present();

        Ok(())
    }
}

fn anyhow_from_string(e: String) -> anyhow::Error {
    anyhow::anyhow!(e)
}
