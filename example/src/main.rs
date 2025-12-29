use std::{collections::BTreeMap, fs};

use anyhow::{Context as _, Result, anyhow};
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
    let config = toml::from_str::<enact::Config>(&config).context("parsing")?;

    let mut bindings_factory = enact::BindingsFactory::new();
    bindings_factory.register::<enact_winit::Input>();
    let (bindings, errors) = bindings_factory.load(&session, &config);
    for error in errors {
        eprintln!("{:?}", error);
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
    bindings: enact::Bindings,
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
        enact_winit::handle(&event, &self.bindings, &mut self.seat);
    }

    fn device_event(
        &mut self,
        event_loop: &winit::event_loop::ActiveEventLoop,
        device_id: winit::event::DeviceId,
        event: winit::event::DeviceEvent,
    ) {
        enact_winit::handle(&event, &self.bindings, &mut self.seat);
    }
}
