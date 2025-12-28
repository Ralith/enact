#[cfg(feature = "serde")]
use serde::{Deserialize, Serialize};
use winit::{
    event::{DeviceEvent, ElementState, KeyEvent, MouseButton, WindowEvent},
    keyboard::PhysicalKey,
};

/// Identifies a source of input data
// TODO: Handwrite better serde impl, winit's sucks
#[derive(Debug, Clone, PartialEq, PartialOrd, Eq, Ord, Hash)]
#[non_exhaustive]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub enum Input {
    PhysicalKeyHeld(PhysicalKey),
    MouseButtonHeld(MouseButton),
    PhysicalKeyPressed(PhysicalKey),
    MouseButtonPressed(MouseButton),
    MouseMotion,
}

impl Input {
    /// Look up the [`Input`]s produced by a [`WindowEvent`]
    pub fn from_window(event: &WindowEvent) -> Vec<Self> {
        match *event {
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

    /// Look up the [`Input`]s produced by a [`DeviceEvent`]
    pub fn from_device(event: &DeviceEvent) -> Vec<Self> {
        match *event {
            DeviceEvent::MouseMotion { .. } => vec![Input::MouseMotion],
            _ => vec![],
        }
    }

    /// Returns `Some` iff `self` can be expressed as an input of type `T`
    pub fn filter<T: InputType>(self) -> Option<Self> {
        T::filter_input(self)
    }

    /// Propagate data from `event` bound via `self` to `action` for `seat`
    pub fn apply_window(
        &self,
        event: &WindowEvent,
        bindings: &enact::Bindings,
        seat: &mut enact::Seat,
    ) {
        match event {
            WindowEvent::KeyboardInput { event, .. } if !event.repeat => match *self {
                Input::PhysicalKeyHeld(_) => {
                    bindings
                        .handle(self, event.state.is_pressed(), seat)
                        .unwrap();
                }
                Input::PhysicalKeyPressed(_) if event.state.is_pressed() => {
                    bindings.handle(self, (), seat).unwrap();
                }
                _ => {}
            },
            WindowEvent::MouseInput { state, .. } => match *self {
                Input::MouseButtonHeld(_) => {
                    bindings.handle(self, state.is_pressed(), seat).unwrap();
                }
                Input::MouseButtonPressed(_) => {
                    bindings.handle(self, (), seat).unwrap();
                }
                _ => {}
            },
            _ => {}
        }
    }

    pub fn apply_device(
        &self,
        event: &DeviceEvent,
        bindings: &enact::Bindings,
        seat: &mut enact::Seat,
    ) {
        match event {
            DeviceEvent::MouseMotion { delta: (x, y) } => match *self {
                Input::MouseMotion => {
                    bindings
                        .handle(self, mint::Vector2::<f64>::from([*x, *y]), seat)
                        .unwrap();
                }
                _ => {}
            },
            _ => {}
        }
    }
}

impl enact::Input for Input {
    fn visit_type<V: enact::InputTypeVisitor>(&self) -> V::Output {
        match *self {
            Input::PhysicalKeyHeld(_) | Input::MouseButtonHeld(_) => V::visit::<bool>(),
            Input::PhysicalKeyPressed(_) | Input::MouseButtonPressed(_) => V::visit::<()>(),
            Input::MouseMotion => V::visit::<mint::Vector2<f64>>(),
        }
    }
}

pub trait InputType {
    fn filter_input(input: Input) -> Option<Input>;
}

impl InputType for () {
    fn filter_input(input: Input) -> Option<Input> {
        match input {
            Input::PhysicalKeyPressed(_) | Input::MouseButtonPressed(_) => Some(input),
            _ => None,
        }
    }
}

impl InputType for bool {
    fn filter_input(input: Input) -> Option<Input> {
        match input {
            Input::PhysicalKeyHeld(_) | Input::MouseButtonHeld(_) => Some(input),
            _ => None,
        }
    }
}

impl InputType for mint::Vector2<f64> {
    fn filter_input(input: Input) -> Option<Input> {
        match input {
            Input::MouseMotion => Some(input),
            _ => None,
        }
    }
}
