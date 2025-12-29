use std::{
    fs,
    sync::Arc,
    time::{Duration, Instant},
};

use anyhow::{Context as _, Result};
use enact::Action;
use winit::{
    application::ApplicationHandler,
    event::{StartCause, WindowEvent},
    event_loop::OwnedDisplayHandle,
    window::{Window, WindowAttributes},
};

fn main() {
    if let Err(e) = run() {
        eprintln!("{:#}", e);
    }
}

fn run() -> Result<()> {
    let mut session = enact::Session::new();
    let actions = Actions::new(&mut session);

    let config = fs::read_to_string("config/seat1.toml").context("reading seat1.toml")?;
    let config = toml::from_str::<enact::Config>(&config).context("parsing")?;

    let mut bindings_factory = enact::BindingsFactory::new();
    bindings_factory.register_source::<enact_winit::Input>();
    let (bindings, errors) = bindings_factory.load(&mut session, &config);
    for error in errors {
        eprintln!("{:?}", error);
    }

    let event_loop = winit::event_loop::EventLoop::new()?;
    let softbuffer = softbuffer::Context::new(event_loop.owned_display_handle()).unwrap();

    let mut app = App {
        bindings,
        seat: enact::Seat::new(),
        window: None,
        actions,

        softbuffer,
        surface: None,
    };
    event_loop.run_app(&mut app)?;

    Ok(())
}

struct Actions {
    direction: Action<mint::Vector2<f64>>,
    jump: Action<()>,
}

impl Actions {
    fn new(session: &mut enact::Session) -> Self {
        Self {
            direction: session.create_action("direction").unwrap(),
            jump: session.create_action("jump").unwrap(),
        }
    }

    fn poll(&self, seat: &enact::Seat) {
        println!(
            "{:1.0?}",
            seat.get(self.direction).map_or([0.0; 2], Into::into)
        );
        if seat.poll(self.jump).is_some() {
            println!("jump");
        }
    }
}

struct App {
    bindings: enact::Bindings,
    seat: enact::Seat,
    window: Option<Arc<Window>>,
    actions: Actions,

    softbuffer: softbuffer::Context<OwnedDisplayHandle>,
    surface: Option<softbuffer::Surface<OwnedDisplayHandle, Arc<Window>>>,
}

impl ApplicationHandler for App {
    fn resumed(&mut self, event_loop: &winit::event_loop::ActiveEventLoop) {
        let window = Arc::new(
            event_loop
                .create_window(WindowAttributes::default())
                .unwrap(),
        );
        self.window = Some(window.clone());
        self.surface = Some(softbuffer::Surface::new(&self.softbuffer, window).unwrap());
        event_loop.set_control_flow(winit::event_loop::ControlFlow::WaitUntil(
            Instant::now() + Duration::from_millis(100),
        ));
    }

    fn new_events(
        &mut self,
        event_loop: &winit::event_loop::ActiveEventLoop,
        cause: winit::event::StartCause,
    ) {
        match cause {
            StartCause::ResumeTimeReached {
                requested_resume, ..
            } => {
                self.bindings.filter(&mut self.seat);
                self.actions.poll(&self.seat);
                self.seat.flush();
                event_loop.set_control_flow(winit::event_loop::ControlFlow::WaitUntil(
                    requested_resume + Duration::from_millis(100),
                ));
            }
            _ => {}
        }
    }

    fn window_event(
        &mut self,
        event_loop: &winit::event_loop::ActiveEventLoop,
        _: winit::window::WindowId,
        event: WindowEvent,
    ) {
        enact_winit::handle(&event, &self.bindings, &mut self.seat);
        match event {
            WindowEvent::CloseRequested => {
                event_loop.exit();
            }
            WindowEvent::Resized(size) => {
                self.surface
                    .as_mut()
                    .unwrap()
                    .resize(
                        size.width.try_into().unwrap(),
                        size.height.try_into().unwrap(),
                    )
                    .unwrap();
            }
            WindowEvent::RedrawRequested => {
                let window = self.window.as_ref().unwrap();
                window.pre_present_notify();

                let surface = self.surface.as_mut().unwrap();
                let mut buffer = surface.buffer_mut().unwrap();
                buffer.fill(0);
                buffer.present().unwrap();
            }
            _ => {}
        }
    }

    fn device_event(
        &mut self,
        _: &winit::event_loop::ActiveEventLoop,
        _: winit::event::DeviceId,
        event: winit::event::DeviceEvent,
    ) {
        enact_winit::handle(&event, &self.bindings, &mut self.seat);
    }
}
