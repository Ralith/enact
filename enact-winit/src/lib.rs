use enact::{ActionId, Seat, Session};
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
        session: &Session,
        event: &WindowEvent,
        action: ActionId,
        seat: &mut Seat,
    ) {
        match event {
            WindowEvent::KeyboardInput { event, .. } if !event.repeat => match *self {
                Input::PhysicalKeyHeld(_) => {
                    let action = session.action::<bool>(action).unwrap();
                    seat.push(action, event.state.is_pressed());
                }
                Input::PhysicalKeyPressed(_) if event.state.is_pressed() => {
                    let action = session.action::<()>(action).unwrap();
                    seat.push(action, ());
                }
                _ => {}
            },
            WindowEvent::MouseInput { state, .. } => match *self {
                Input::MouseButtonHeld(_) => {
                    let action = session.action::<bool>(action).unwrap();
                    seat.push(action, state.is_pressed());
                }
                Input::MouseButtonPressed(_) => {
                    let action = session.action::<()>(action).unwrap();
                    seat.push(action, ());
                }
                _ => {}
            },
            _ => {}
        }
    }

    pub fn apply_device(
        &self,
        session: &Session,
        event: &DeviceEvent,
        action: ActionId,
        seat: &mut Seat,
    ) {
        match event {
            DeviceEvent::MouseMotion { delta: (x, y) } => match *self {
                Input::MouseMotion => {
                    let action = session.action::<mint::Vector2<f64>>(action).unwrap();
                    seat.push(action, [*x, *y].into());
                }
                _ => {}
            },
            _ => {}
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
