use std::{collections::BTreeMap, fs};

use anyhow::{Context as _, Result, anyhow};
use rustc_hash::FxHashMap;
use winit::{
    application::ApplicationHandler,
    window::{Window, WindowAttributes},
};

fn main() {
    if let Err(e) = run() {
        eprintln!("{:#}", e);
    }
}

fn run() -> Result<()> {
    let mut session = enact::Session::new();
    let [up, left, down, right] =
        ["up", "left", "down", "right"].map(|name| session.create_action::<()>(name));

    let config = fs::read_to_string("config/seat1.toml").context("reading seat1.toml")?;
    let config =
        toml::from_str::<BTreeMap<String, Vec<enact_winit::Input>>>(&config).context("parsing")?;

    // TODO: Factor out
    let mut bindings = FxHashMap::<enact_winit::Input, Vec<enact::ActionId>>::default();
    for (name, inputs) in config.into_iter() {
        let action = session
            .action_id(&name)
            .ok_or_else(|| anyhow!("unknown action {name}"))?;
        for input in inputs {
            bindings.entry(input).or_default().push(action);
        }
    }

    let mut app = App {
        bindings,
        session,
        seat: enact::Seat::new(),
        window: None,
    };

    let event_loop = winit::event_loop::EventLoop::new()?;
    event_loop.run_app(&mut app)?;

    Ok(())
}

struct App {
    session: enact::Session,
    bindings: FxHashMap<enact_winit::Input, Vec<enact::ActionId>>,
    seat: enact::Seat,
    window: Option<Window>,
}

impl ApplicationHandler for App {
    fn resumed(&mut self, event_loop: &winit::event_loop::ActiveEventLoop) {
        self.window = Some(
            event_loop
                .create_window(WindowAttributes::default())
                .unwrap(),
        );
    }

    fn window_event(
        &mut self,
        event_loop: &winit::event_loop::ActiveEventLoop,
        window_id: winit::window::WindowId,
        event: winit::event::WindowEvent,
    ) {
        // TODO: Factor out
        for input in enact_winit::Input::from_window(&event) {
            for &action in self.bindings.get(&input).map_or(&[][..], Vec::as_slice) {
                input.apply_window(&self.session, &event, action, &mut self.seat);
            }
        }
    }

    fn device_event(
        &mut self,
        event_loop: &winit::event_loop::ActiveEventLoop,
        device_id: winit::event::DeviceId,
        event: winit::event::DeviceEvent,
    ) {
        for input in enact_winit::Input::from_device(&event) {
            for &action in self.bindings.get(&input).map_or(&[][..], Vec::as_slice) {
                input.apply_device(&self.session, &event, action, &mut self.seat);
            }
        }
    }
}
