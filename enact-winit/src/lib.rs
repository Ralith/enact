use winit::{
    event::{DeviceEvent, ElementState, KeyEvent, MouseButton, WindowEvent},
    keyboard::{KeyCode, NativeKeyCode, PhysicalKey},
};

/// Identifies a source of input data
#[derive(Debug, Clone, PartialEq, PartialOrd, Eq, Ord, Hash)]
#[non_exhaustive]
pub enum Input {
    PhysicalKeyHeld(PhysicalKey),
    MouseButtonHeld(MouseButton),
    PhysicalKeyPressed(PhysicalKey),
    MouseButtonPressed(MouseButton),
    MouseMotion,
}

impl Input {
    /// Look up the [`Input`]s produced by a winit event
    ///
    /// Useful for building binding UIs. Call [`enact::Session::check_type`] to
    /// filter out inputs which are inappropriate for a specific action.
    ///
    /// Convenience wrapper for [`Event::to_inputs`]
    pub fn from_event<E: Event>(event: &E) -> Vec<Self> {
        event.to_inputs()
    }
}

impl enact::Input for Input {
    const NAME: &'static str = "winit";

    fn visit_type<V: enact::InputTypeVisitor>(&self) -> V::Output {
        match *self {
            Input::PhysicalKeyHeld(_) | Input::MouseButtonHeld(_) => V::visit::<bool>(),
            Input::PhysicalKeyPressed(_) | Input::MouseButtonPressed(_) => V::visit::<()>(),
            Input::MouseMotion => V::visit::<mint::Vector2<f64>>(),
        }
    }

    fn from_str(s: &str) -> Vec<Self> {
        if let Some(key) = parse_key(s) {
            return vec![Input::PhysicalKeyHeld(key), Input::PhysicalKeyPressed(key)];
        }
        if let Some(button) = parse_mouse_button(s) {
            return vec![
                Input::MouseButtonHeld(button),
                Input::MouseButtonPressed(button),
            ];
        }
        vec![]
    }

    fn to_string(&self) -> String {
        match *self {
            Input::PhysicalKeyHeld(k) | Input::PhysicalKeyPressed(k) => format_key(k),
            Input::MouseButtonHeld(b) | Input::MouseButtonPressed(b) => format_mouse_button(b),
            Input::MouseMotion => "mouse".to_owned(),
        }
    }
}

fn parse_mouse_button(x: &str) -> Option<MouseButton> {
    Some(match &*x.to_ascii_lowercase() {
        "mouse left" => MouseButton::Left,
        "mouse right" => MouseButton::Right,
        "mouse middle" => MouseButton::Middle,
        "mouse back" => MouseButton::Back,
        "mouse forward" => MouseButton::Forward,
        other => {
            if let Some(suffix) = other.strip_prefix("mouse ") {
                MouseButton::Other(suffix.parse().ok()?)
            } else {
                return None;
            }
        }
    })
}

fn format_mouse_button(x: MouseButton) -> String {
    match x {
        MouseButton::Left => "mouse left",
        MouseButton::Right => "mouse right",
        MouseButton::Middle => "mouse middle",
        MouseButton::Back => "mouse back",
        MouseButton::Forward => "mouse forward",
        MouseButton::Other(n) => return format!("mouse {n}"),
    }
    .to_owned()
}

fn parse_key(x: &str) -> Option<PhysicalKey> {
    if let Some(code) = parse_keycode(x) {
        return Some(PhysicalKey::Code(code));
    }
    let Some(x) = x.strip_prefix("<") else {
        return None;
    };
    for (id, f) in [
        (
            "android",
            NativeKeyCode::Android as fn(u32) -> NativeKeyCode,
        ),
        ("macos", |n| NativeKeyCode::MacOS(n as u16)),
        ("windows", |n| NativeKeyCode::Windows(n as u16)),
        ("xkb", NativeKeyCode::Xkb),
    ] {
        let Some(x) = x.strip_prefix(id) else {
            continue;
        };
        let Some(x) = x.strip_prefix(' ') else {
            continue;
        };
        let Some(x) = x.strip_suffix('>') else {
            continue;
        };
        let Ok(x) = x.parse::<u32>() else {
            continue;
        };
        return Some(PhysicalKey::Unidentified(f(x)));
    }
    None
}

fn format_key(k: PhysicalKey) -> String {
    match k {
        PhysicalKey::Code(k) => format_keycode(k).to_owned(),
        PhysicalKey::Unidentified(k) => match k {
            NativeKeyCode::Unidentified => "<unknown>".to_owned(),
            NativeKeyCode::Android(n) => format!("<android {n}>"),
            NativeKeyCode::MacOS(n) => format!("<macos {n}>"),
            NativeKeyCode::Windows(n) => format!("<windows {n}>"),
            NativeKeyCode::Xkb(n) => format!("<xkb {n}>"),
        },
    }
}

macro_rules! keycodes {
    ($($variant:ident => $s:literal,)*) => {
        fn parse_keycode(x: &str) -> Option<KeyCode> {
            use KeyCode::*;
            Some(match &*x.to_ascii_lowercase() {
                $($s => $variant,)*
                _ => return None,
            })
        }

        fn format_keycode(x: KeyCode) -> &'static str {
            use KeyCode::*;
            match x {
                $($variant => $s,)*
                _ => todo!(),
            }
        }
    };
}

keycodes! {
    KeyW => "w",
    KeyA => "a",
    KeyS => "s",
    KeyD => "d",
}

/// Update action states in `seat` to account for any inputs in `event`
/// according to `bindings`
///
/// Convenience wrapper for [`Event::handle`]
pub fn handle<E: Event>(event: &E, bindings: &enact::Bindings, seat: &mut enact::Seat) {
    event.handle(bindings, seat);
}

/// Winit events that might contain supported inputs
pub trait Event {
    /// See [`handle`]
    fn handle(&self, bindings: &enact::Bindings, seat: &mut enact::Seat);

    /// See [`Input::from_event`]
    fn to_inputs(&self) -> Vec<Input>;
}

impl Event for WindowEvent {
    fn handle(&self, bindings: &enact::Bindings, seat: &mut enact::Seat) {
        match *self {
            WindowEvent::KeyboardInput { ref event, .. } if !event.repeat => {
                bindings
                    .handle(
                        &Input::PhysicalKeyHeld(event.physical_key),
                        event.state.is_pressed(),
                        seat,
                    )
                    .unwrap();
                bindings
                    .handle(&Input::PhysicalKeyPressed(event.physical_key), (), seat)
                    .unwrap();
            }
            WindowEvent::MouseInput { state, button, .. } => {
                bindings
                    .handle(&Input::MouseButtonHeld(button), state.is_pressed(), seat)
                    .unwrap();

                bindings
                    .handle(&Input::MouseButtonPressed(button), (), seat)
                    .unwrap();
            }
            _ => {}
        }
    }

    fn to_inputs(&self) -> Vec<Input> {
        match *self {
            WindowEvent::KeyboardInput {
                event:
                    KeyEvent {
                        physical_key,
                        state: ElementState::Pressed,
                        ..
                    },
                is_synthetic: false,
                ..
            } => vec![
                Input::PhysicalKeyPressed(physical_key),
                Input::PhysicalKeyHeld(physical_key),
            ],
            WindowEvent::MouseInput {
                button,
                state: ElementState::Pressed,
                ..
            } => vec![
                Input::MouseButtonPressed(button),
                Input::MouseButtonHeld(button),
            ],
            _ => vec![],
        }
    }
}

impl Event for DeviceEvent {
    fn handle(&self, bindings: &enact::Bindings, seat: &mut enact::Seat) {
        match *self {
            DeviceEvent::MouseMotion { delta: (x, y) } => {
                bindings
                    .handle(
                        &Input::MouseMotion,
                        mint::Vector2::<f64>::from([x, y]),
                        seat,
                    )
                    .unwrap();
            }
            _ => {}
        }
    }

    fn to_inputs(&self) -> Vec<Input> {
        match *self {
            DeviceEvent::MouseMotion { .. } => vec![Input::MouseMotion],
            _ => vec![],
        }
    }
}

impl<T> Event for winit::event::Event<T> {
    fn handle(&self, bindings: &enact::Bindings, seat: &mut enact::Seat) {
        use winit::event::Event::*;
        match *self {
            WindowEvent { ref event, .. } => handle(event, bindings, seat),
            DeviceEvent { ref event, .. } => handle(event, bindings, seat),
            _ => {}
        }
    }

    fn to_inputs(&self) -> Vec<Input> {
        use winit::event::Event::*;
        match *self {
            WindowEvent { ref event, .. } => event.to_inputs(),
            DeviceEvent { ref event, .. } => event.to_inputs(),
            _ => vec![],
        }
    }
}
