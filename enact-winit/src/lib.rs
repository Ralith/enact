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

    /// Returns `Some` iff `self` can be expressed as an input of type `T`
    pub fn filter<T: InputType>(self) -> Option<Self> {
        T::filter_input(self)
    }

    /// Propagate data from `event` bound via `self` to `action` for `seat`
    fn apply_window(
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

    fn apply_device(
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

pub fn handle<E: Event>(event: &E, bindings: &enact::Bindings, seat: &mut enact::Seat) {
    event.handle(bindings, seat);
}

pub trait Event {
    fn handle(&self, bindings: &enact::Bindings, seat: &mut enact::Seat);
    fn to_inputs(&self) -> Vec<Input>;
}

impl Event for WindowEvent {
    fn handle(&self, bindings: &enact::Bindings, seat: &mut enact::Seat) {
        for input in self.to_inputs() {
            input.apply_window(self, bindings, seat);
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
        for input in self.to_inputs() {
            input.apply_device(self, bindings, seat);
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
