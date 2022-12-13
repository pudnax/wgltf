use std::time::Instant;

use color_eyre::Result;
use wgltf::utils::Input;
use wgpu::SurfaceError;
use winit::{
    dpi::{PhysicalPosition, PhysicalSize},
    event::{DeviceEvent, Event, KeyboardInput, MouseScrollDelta, VirtualKeyCode, WindowEvent},
    event_loop::{ControlFlow, EventLoop},
    window::WindowBuilder,
};

const UPDATES_PER_SECOND: i32 = 60;
const MAX_FRAME_TIME: f64 = 0.1;
const FIXED_TIME_STEP: f64 = 1. / UPDATES_PER_SECOND as f64;

fn main() -> Result<()> {
    let event_loop = EventLoop::new();
    let window = WindowBuilder::new().build(&event_loop)?;

    let mut state = wgltf::State::new(&window)?;
    println!("{}", state.get_info());

    let mut input = Input::new();
    let zoom_speed = 0.002;
    let rotate_speed = 0.0025;

    let mut frame_number = 0;
    let mut previous_instant = Instant::now();
    let mut _blending_factor = 0.;
    let mut accumulated_time = 0.;
    let mut timeline = 0.;

    event_loop.run(move |event, _, control_flow| {
        control_flow.set_poll();
        match event {
            Event::MainEventsCleared => {
                window.request_redraw();
            }
            Event::WindowEvent {
                event:
                    WindowEvent::Resized(PhysicalSize { width, height })
                    | WindowEvent::ScaleFactorChanged {
                        new_inner_size: &mut PhysicalSize { width, height },
                        ..
                    },
                ..
            } => {
                if width != 0 && height != 0 {
                    state.resize(width, height);
                }
            }
            Event::RedrawRequested(_) => {
                let current_instant = Instant::now();
                let mut elapsed = current_instant
                    .duration_since(previous_instant)
                    .as_secs_f64();
                if elapsed > MAX_FRAME_TIME {
                    elapsed = MAX_FRAME_TIME;
                }
                accumulated_time += elapsed;
                timeline += elapsed;
                while accumulated_time >= FIXED_TIME_STEP {
                    state.update(timeline, frame_number);

                    accumulated_time -= FIXED_TIME_STEP;
                    frame_number += 1;
                }
                _blending_factor = accumulated_time / FIXED_TIME_STEP;
                if let Err(err) = state.render_mesh() {
                    eprintln!("get_current_texture error: {:?}", err);
                    match err {
                        SurfaceError::Lost | SurfaceError::Outdated => {
                            state
                                .surface
                                .configure(&state.device, &state.surface_config);
                            window.request_redraw();
                        }
                        SurfaceError::OutOfMemory => {
                            *control_flow = ControlFlow::Exit;
                        }
                        _ => (),
                    }
                };

                previous_instant = current_instant;
            }
            Event::DeviceEvent { event, .. } => match event {
                DeviceEvent::MouseWheel { delta, .. } => {
                    let scroll_amount = -match delta {
                        MouseScrollDelta::LineDelta(_, scroll) => scroll,
                        MouseScrollDelta::PixelDelta(PhysicalPosition { y: scroll, .. }) => {
                            scroll as f32
                        }
                    };
                    state.camera.add_zoom(scroll_amount * zoom_speed);
                }
                DeviceEvent::MouseMotion { delta } => {
                    if input.left_mouse_pressed {
                        state.camera.add_yaw(-delta.0 as f32 * rotate_speed);
                        state.camera.add_pitch(delta.1 as f32 * rotate_speed);
                    }
                }
                _ => {}
            },
            Event::WindowEvent {
                event:
                    WindowEvent::CloseRequested
                    | WindowEvent::KeyboardInput {
                        input:
                            KeyboardInput {
                                virtual_keycode: Some(VirtualKeyCode::Escape),
                                ..
                            },
                        ..
                    },
                ..
            } => *control_flow = ControlFlow::Exit,
            Event::WindowEvent { event, .. } => {
                input.update(&event, &window);
            }
            _ => {}
        }
    })
}
