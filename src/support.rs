use glium::glutin;
use glium::glutin::event::{Event, WindowEvent};
use glium::glutin::event_loop::{ControlFlow, EventLoop};
use glium::glutin::window::{Icon, WindowBuilder};
use glium::{Display, Surface};
// use image::GenericImageView;
use imgui::{ConfigFlags, Context, FontConfig, FontGlyphRanges, FontSource};
use imgui_glium_renderer::Renderer;
use imgui_winit_support::{HiDpiMode, WinitPlatform};
use std::path::Path;
use std::time::Instant;

use crate::{app::App, clipboard};

pub struct System {
    pub event_loop: EventLoop<()>,
    pub display: glium::Display,
    pub imgui: Context,
    pub platform: WinitPlatform,
    pub renderer: Renderer,
}

fn load_icon() -> Option<Icon> {
    let buffer = include_bytes!("../resources/icons8-magnifying-glass-tilted-left-96.png");
    if let Ok(img) = crate::stb_image::load_bytes(buffer.as_ref()) {
        let rgba_bytes = img.data().to_vec();
        return Icon::from_rgba(rgba_bytes, img.width, img.height).ok();
    } else {
        return None;
    }
}

pub fn init(title: &str) -> System {
    let title = match Path::new(&title).file_name() {
        Some(file_name) => file_name.to_str().unwrap(),
        None => title,
    };
    let event_loop = EventLoop::new();
    let context = glutin::ContextBuilder::new().with_vsync(true);
    let builder = WindowBuilder::new()
        .with_title(title.to_owned())
        .with_inner_size(glutin::dpi::LogicalSize::new(1024f64, 768f64))
        .with_window_icon(load_icon());
    let display =
        Display::new(builder, context, &event_loop).expect("Failed to initialize display");

    let mut imgui = Context::create();
    imgui.set_ini_filename(None);

    if let Some(backend) = clipboard::init() {
        imgui.set_clipboard_backend(backend);
    } else {
        eprintln!("Failed to initialize clipboard");
    }

    let mut platform = WinitPlatform::init(&mut imgui);
    {
        let gl_window = display.gl_window();
        let window = gl_window.window();

        let dpi_mode = if let Ok(factor) = std::env::var("IMGUI_EXAMPLE_FORCE_DPI_FACTOR") {
            // Allow forcing of HiDPI factor for debugging purposes
            match factor.parse::<f64>() {
                Ok(f) => HiDpiMode::Locked(f),
                Err(e) => panic!("Invalid scaling factor: {}", e),
            }
        } else {
            HiDpiMode::Default
        };

        platform.attach_window(imgui.io_mut(), window, dpi_mode);
    }

    let hidpi_factor = platform.hidpi_factor() as f32 ;

    imgui.fonts().add_font(&[
        FontSource::TtfData {
            data: include_bytes!("../resources/Lucon.ttf"),
            size_pixels: 12.0 * hidpi_factor,
            config: Some(FontConfig {
                // As imgui-glium-renderer isn't gamma-correct with it's font rendering,
                // we apply an arbitrary multiplier to make the font a bit "heavier".
                // With default imgui-glow-renderer this is unnecessary.
                rasterizer_multiply: 1.2,
                // Oversampling font helps improve text rendering at expense of larger
                // font atlas texture.
                oversample_h: 4,
                oversample_v: 4,
                ..FontConfig::default()
            }),
        },
        FontSource::TtfData {
            data: include_bytes!("../resources/mplus-1p-regular.ttf"),
            size_pixels: 15.0 * hidpi_factor,
            config: Some(FontConfig {
                // Oversampling font helps improve text rendering at expense of larger
                // font atlas texture.
                oversample_h: 4,
                oversample_v: 4,
                // Range of glyphs to rasterize
                glyph_ranges: FontGlyphRanges::japanese(),
                ..FontConfig::default()
            }),
        },
    ]);

    // @Cleanup:
    // This is apprently necessary on MacOS, because it pretend it has 2x less pixel
    // than it actually does, so the trick is to rasterize the font twice as big and
    // scale it down in order to have font of the right size, but crisp looking.
    //
    // Can somebody test??
    //
    // imgui.io_mut().font_global_scale = (1.0 / hidpi_factor) as f32;

    imgui.style_mut().scale_all_sizes(hidpi_factor);

    let renderer = Renderer::init(&mut imgui, &display).expect("Failed to initialize renderer");

    return System {
        event_loop,
        display,
        imgui,
        platform,
        renderer,
    };
}

impl System {
    pub fn main_loop(self, mut app: App) {
        let System {
            event_loop,
            display,
            mut imgui,
            mut platform,
            mut renderer,
            ..
        } = self;

        // Allow us to use PageUp and PageDown to navigate in the result window.
        imgui
            .io_mut()
            .config_flags
            .set(ConfigFlags::NAV_ENABLE_KEYBOARD, true);

        let mut last_frame = Instant::now();
        event_loop.run(move |event, _, control_flow| match event {
            Event::NewEvents(_) => {
                let now = Instant::now();
                imgui.io_mut().update_delta_time(now - last_frame);
                last_frame = now;
            }
            Event::MainEventsCleared => {
                let gl_window = display.gl_window();
                platform
                    .prepare_frame(imgui.io_mut(), gl_window.window())
                    .expect("Failed to prepare frame");
                gl_window.window().request_redraw();
            }
            Event::RedrawRequested(_) => {
                let ui = imgui.frame();

                let mut run = true;
                app.update(&mut run, ui);
                if !run {
                    *control_flow = ControlFlow::Exit;
                }

                let gl_window = display.gl_window();
                let mut target = display.draw();
                target.clear_color_srgb(1.0, 1.0, 1.0, 1.0);
                platform.prepare_render(ui, gl_window.window());
                let draw_data = imgui.render();
                renderer
                    .render(&mut target, draw_data)
                    .expect("Rendering failed");
                target.finish().expect("Failed to swap buffers");

                app.process_drag_drop(imgui.io_mut());
            }
            Event::WindowEvent {
                event: WindowEvent::CloseRequested,
                ..
            } => *control_flow = ControlFlow::Exit,
            Event::WindowEvent {
                event: WindowEvent::Resized(new_size),
                ..
            } => imgui.io_mut().display_size = [new_size.width as f32, new_size.height as f32],
            /*
            // @Cleanup:
            // When testing on a Windows machine with hidpi (scaling factor 2), the mouse pos was
            // multiplied by two. This seem to fix it, but might break on other platform? It would
            // be something in "imgui-winit-support" crate, but not sure what yet.
            //
            // How does it look on other OS?
            Event::WindowEvent {
                event: WindowEvent::CursorMoved { position, ..},
                ..
            } => imgui.io_mut().add_mouse_pos_event([position.x as f32, position.y as f32]),
            */
            event => {
                let gl_window = display.gl_window();
                if !app.handle_event(gl_window.window(), &event) {
                    platform.handle_event(imgui.io_mut(), gl_window.window(), &event);
                }
            }
        });
    }
}
