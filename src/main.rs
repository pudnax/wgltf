use color_eyre::Result;
use wgltf::utils::{FrameCounter, Input};
use wgpu::SurfaceError;
use winit::{
    dpi::{PhysicalPosition, PhysicalSize},
    event::{DeviceEvent, Event, KeyboardInput, MouseScrollDelta, VirtualKeyCode, WindowEvent},
    event_loop::{ControlFlow, EventLoop},
    window::WindowBuilder,
};

fn main() -> Result<()> {
    let event_loop = EventLoop::new();
    let window = WindowBuilder::new().build(&event_loop)?;

    let mut state = wgltf::State::new(&window)?;
    println!("{}", state.get_info());

    let mut input = Input::new();
    let zoom_speed = 0.002;
    let rotate_speed = 0.0025;
    let mut frame_counter = FrameCounter::new();

    event_loop.run(move |event, _, control_flow| {
        control_flow.set_poll();
        match event {
            Event::MainEventsCleared => {
                state.update(&frame_counter);
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
                frame_counter.record();
                if let Err(err) = state.render() {
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
