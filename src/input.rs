use glium::glutin;

#[derive(Debug, Default)]
pub struct Input {
    /// Some if currently in a drag action, the start x, start y, and if
    /// the drag was released since the last frame
    /// TODO: add start time and input button to sense clicks/taps
    pub pointer_drag : Option<(f32, f32, bool)>,
    pub pointer : (f32, f32),
    pub scroll_delta : (f32, f32),
    pub modifiers : glutin::event::ModifiersState,
}

impl Input {
    crate fn frame_reset(&mut self) {
        if matches!(self.pointer_drag, Some((_, _, true))) {
            self.pointer_drag = None;
        }

        self.scroll_delta = (0.0, 0.0);
    }

    crate fn update(&mut self, evt : glutin::event::WindowEvent<'_>) {
        use glutin::event::WindowEvent::*;
        use glutin::event::ElementState;
        use glutin::event::MouseScrollDelta;

        use glutin::dpi::PhysicalPosition;

        match evt {
            CursorMoved{position, ..} => {
                self.pointer = (position.x as f32, position.y as f32);
            },
            MouseInput{state, ..} => {
                match state {
                    ElementState::Pressed => {
                        self.pointer_drag = Some((
                            self.pointer.0,
                            self.pointer.1,
                            false,
                        ));
                    },
                    ElementState::Released => {
                        self.pointer_drag.iter_mut().for_each(|(_, _, released)| {
                            *released = true;
                        })
                    }
                }
            },
            ModifiersChanged(modifiers) => {
                self.modifiers = modifiers;
            },
            MouseWheel{ delta, ..} => {
                match delta {
                    MouseScrollDelta::LineDelta(x, y) => {
                        // TODO: test this code path
                        self.scroll_delta.0 += x;
                        self.scroll_delta.1 += y;
                    },
                    MouseScrollDelta::PixelDelta(PhysicalPosition{x, y}) => {
                        self.scroll_delta.0 -= x as f32;
                        self.scroll_delta.1 -= y as f32;
                    }
                }
            }
            _ => {},
        }
    }

    pub fn drag_delta(&self) -> Option<(f32, f32, bool)> {
        let (x1, y1) = self.pointer;

        self.pointer_drag.map(|(x0,y0, released)| {
            (x1 - x0, y0 - y1, released)
        })
    }
}
