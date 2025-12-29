#[cfg(feature = "serde")]
use serde::Deserialize;
use winit::{
    event::{DeviceEvent, ElementState, KeyEvent, MouseButton, WindowEvent},
    keyboard::PhysicalKey,
};

/// Identifies a source of input data
#[derive(Debug, Clone, PartialEq, PartialOrd, Eq, Ord, Hash)]
#[non_exhaustive]
#[cfg_attr(feature = "serde", derive(Deserialize))]
pub enum Input {
    PhysicalKeyHeld(PhysicalKey),
    MouseButtonHeld(MouseButton),
    PhysicalKeyPressed(PhysicalKey),
    MouseButtonPressed(MouseButton),
    MouseMotion,
}

impl Input {
    /// Look up the [`Input`]s produced by a winit event
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

    fn to_string(&self) -> String {
        format!("{self:?}")
    }
}

pub fn handle<E: Event>(event: &E, bindings: &enact::Bindings, seat: &mut enact::Seat) {
    event.handle(bindings, seat);
}

pub trait Event {
    fn handle(&self, bindings: &enact::Bindings, seat: &mut enact::Seat);
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
