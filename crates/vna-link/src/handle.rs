//! The one handle the GUI holds: [`Cmd`]s in, [`Event`]s out, whether the
//! device is on a serial port of this machine (the worker thread) or behind
//! a vna-server (a [`Session`] over a WebSocket).
//!
//! `Cmd::Connect` picks: an address for which [`is_server_address`] holds
//! opens a session, anything else goes to the worker. One device at a time:
//! a `Connect` lets go of the other backend first, and only the events of
//! the backend in use reach the GUI. The browser has only the session.

use std::cell::RefCell;
use std::collections::VecDeque;
#[cfg(not(target_arch = "wasm32"))]
use std::sync::mpsc::{channel, Receiver, RecvTimeoutError, Sender};
use std::sync::Arc;

use web_time::Instant;

use crate::remote::{not_connected, Opener, RemoteTiming, Session, Socket};
use crate::{is_server_address, server_url, Cmd, Event, CONNECT_FAILED};

/// Called after every event, from whichever thread has it, so that the GUI
/// repaints instead of polling. It must return quickly: during a sweep it
/// runs once per block of data.
pub type Notify = Arc<dyn Fn() + Send + Sync>;

/// What a worker or a session does beyond what one UI on the same machine needs.
#[derive(Debug, Clone, Copy, Default)]
pub struct Options {
    /// Send [`Event::Continuous`] whenever it changes whether continuous
    /// sweeping is on (or will resume when a lost link is back). A server
    /// sets this, so that all its clients know; a local UI knows what it
    /// asked for and leaves it off.
    pub announce_continuous: bool,
    /// How a session with a server waits and retries.
    pub remote: RemoteTiming,
}

/// The handle for a local UI: [`spawn_with`] with the default [`Options`].
pub fn spawn(notify: Notify) -> WorkerHandle {
    spawn_with(notify, Options::default())
}

/// `ewebsock` as a [`Socket`]; `wake` is called when it has news.
///
/// The event is queued first and `wake` called after. ewebsock's own receiver
/// (`connect_with_wakeup`) does it the other way round, which suits a GUI that
/// only looks at its next frame, but not a thread that looks the moment it
/// is woken: it could find nothing yet and go back to sleep with the event
/// lying there, until something else woke it. That left the result of a
/// single sweep undelivered now and then.
fn open_ws(url: &str, wake: impl Fn() + Send + Sync + 'static) -> Result<Box<dyn Socket>, String> {
    struct Ws(ewebsock::WsSender, std::sync::mpsc::Receiver<ewebsock::WsEvent>);
    impl Socket for Ws {
        fn send(&mut self, text: String) {
            self.0.send(ewebsock::WsMessage::Text(text));
        }
        fn try_recv(&mut self) -> Option<ewebsock::WsEvent> {
            self.1.try_recv().ok()
        }
    }
    let (events, rx) = std::sync::mpsc::channel();
    let on_event = move |event| {
        if events.send(event).is_err() {
            // The session let go of the socket.
            return std::ops::ControlFlow::Break(());
        }
        wake();
        std::ops::ControlFlow::Continue(())
    };
    let tx = ewebsock::ws_connect(url.to_string(), ewebsock::Options::default(), Box::new(on_event))?;
    Ok(Box::new(Ws(tx, rx)))
}

/// The canonical URL for `port`, or why there is none.
fn url_for(port: &str) -> Result<String, &'static str> {
    let url = server_url(port).ok_or("not a server address")?;
    #[cfg(not(target_arch = "wasm32"))]
    if url.starts_with("wss://") {
        // ewebsock is built without a TLS stack; in a browser wss works.
        return Err("wss:// needs TLS, which this program is built without; connect with ws://, or use the page in a browser");
    }
    Ok(url)
}

/// A `Connect` that fails before anything is tried.
fn cannot_connect(port: &str, why: &str) -> Event {
    Event::Error {
        msg: format!("{CONNECT_FAILED}{port}: {why}"),
        stops_sweep: true,
    }
}

#[cfg(not(target_arch = "wasm32"))]
pub use native::{spawn_with, spawn_worker, WorkerHandle};

#[cfg(target_arch = "wasm32")]
pub use web::{spawn_with, WorkerHandle};

#[cfg(not(target_arch = "wasm32"))]
mod native {
    use super::*;
    use std::cell::Cell;
    use std::thread;
    use std::time::Duration;

    /// The worker thread alone, without the routing of [`WorkerHandle`]:
    /// for a server, which only ever drives its own device.
    pub fn spawn_worker(notify: Notify, options: Options) -> (Sender<Cmd>, Receiver<Event>) {
        crate::worker::start(notify, options)
    }

    pub fn spawn_with(notify: Notify, options: Options) -> WorkerHandle {
        let (local, local_events) = crate::worker::start(notify.clone(), options);
        WorkerHandle {
            notify,
            options,
            local,
            local_events,
            remote: RefCell::new(None),
            remote_active: Cell::new(false),
            own: RefCell::new(VecDeque::new()),
        }
    }

    enum Input {
        Cmd(Cmd),
        /// A socket has news.
        Wake,
        /// The handle let go of the session.
        Close,
    }

    /// A session on its own thread.
    struct Remote {
        inputs: Sender<Input>,
        events: Receiver<Event>,
    }

    impl Drop for Remote {
        fn drop(&mut self) {
            let _ = self.inputs.send(Input::Close);
        }
    }

    pub struct WorkerHandle {
        notify: Notify,
        options: Options,
        local: Sender<Cmd>,
        local_events: Receiver<Event>,
        remote: RefCell<Option<Remote>>,
        /// Commands go to the session (or there is none: the address was bad).
        remote_active: Cell<bool>,
        /// Events the handle makes itself (a bad address).
        own: RefCell<VecDeque<Event>>,
    }

    impl WorkerHandle {
        pub fn send(&self, cmd: Cmd) {
            match &cmd {
                Cmd::Connect(port) if is_server_address(port) => {
                    if !self.remote_active.get() {
                        // Let go of a device on a serial port.
                        let _ = self.local.send(Cmd::Disconnect);
                    }
                    self.remote_active.set(true);
                    *self.remote.borrow_mut() = None;
                    match url_for(port) {
                        Ok(url) => *self.remote.borrow_mut() = Some(self.start(port.clone(), url)),
                        Err(why) => {
                            self.own.borrow_mut().push_back(cannot_connect(port, why));
                            (self.notify)();
                        }
                    }
                }
                Cmd::Connect(_) => {
                    self.remote_active.set(false);
                    *self.remote.borrow_mut() = None;
                    let _ = self.local.send(cmd);
                }
                _ if self.remote_active.get() => match &*self.remote.borrow() {
                    Some(r) => {
                        let _ = r.inputs.send(Input::Cmd(cmd));
                    }
                    None => {
                        self.own.borrow_mut().extend(not_connected(&cmd));
                        (self.notify)();
                    }
                },
                _ => {
                    let _ = self.local.send(cmd);
                }
            }
        }

        /// The next event, if there is one.
        pub fn try_recv(&self) -> Option<Event> {
            if let Some(e) = self.own.borrow_mut().pop_front() {
                return Some(e);
            }
            if self.remote_active.get() {
                // What the worker still says after letting go is not news.
                while self.local_events.try_recv().is_ok() {}
                self.remote.borrow().as_ref().and_then(|r| r.events.try_recv().ok())
            } else {
                self.local_events.try_recv().ok()
            }
        }

        /// Wait for the next event; for tests and programs without a GUI.
        pub fn recv_timeout(&self, timeout: Duration) -> Result<Event, RecvTimeoutError> {
            if let Some(e) = self.own.borrow_mut().pop_front() {
                return Ok(e);
            }
            if self.remote_active.get() {
                while self.local_events.try_recv().is_ok() {}
                match &*self.remote.borrow() {
                    Some(r) => r.events.recv_timeout(timeout),
                    None => {
                        thread::sleep(timeout);
                        Err(RecvTimeoutError::Timeout)
                    }
                }
            } else {
                self.local_events.recv_timeout(timeout)
            }
        }

        fn start(&self, port: String, url: String) -> Remote {
            let (inputs, rx) = channel::<Input>();
            let (events, events_rx) = channel::<Event>();
            let wake = inputs.clone();
            let notify = self.notify.clone();
            let timing = self.options.remote;
            thread::Builder::new()
                .name("vna-server session".into())
                .spawn(move || {
                    let open: Opener = Box::new(move |url| {
                        let wake = wake.clone();
                        open_ws(url, move || {
                            let _ = wake.send(Input::Wake);
                        })
                    });
                    let mut s = Session::start(port, url, open, timing, None, Instant::now());
                    loop {
                        let mut any = false;
                        while let Some(e) = s.next_event() {
                            any = true;
                            if events.send(e).is_err() {
                                return;
                            }
                        }
                        if any {
                            notify();
                        }
                        let wait = s
                            .deadline()
                            .map(|d| d.saturating_duration_since(Instant::now()))
                            .unwrap_or(Duration::from_secs(3600));
                        match rx.recv_timeout(wait) {
                            Ok(Input::Cmd(c)) => s.command(c),
                            Ok(Input::Wake) | Err(RecvTimeoutError::Timeout) => {}
                            Ok(Input::Close) | Err(RecvTimeoutError::Disconnected) => return,
                        }
                        s.poll(Instant::now());
                    }
                })
                .expect("spawn session thread");
            Remote { inputs, events: events_rx }
        }
    }
}

#[cfg(target_arch = "wasm32")]
mod web {
    use super::*;

    /// The browser build: no worker, no thread. The session is driven from
    /// [`WorkerHandle::try_recv`], which the GUI calls every frame; a socket
    /// with news, and a browser timer at the session's deadline, ask the GUI
    /// for a frame through `notify`.
    pub fn spawn_with(notify: Notify, options: Options) -> WorkerHandle {
        WorkerHandle {
            notify,
            options,
            session: RefCell::new(None),
            own: RefCell::new(VecDeque::new()),
            timer_for: RefCell::new(None),
        }
    }

    pub struct WorkerHandle {
        notify: Notify,
        options: Options,
        session: RefCell<Option<Session>>,
        own: RefCell<VecDeque<Event>>,
        /// The deadline a timer is set for.
        timer_for: RefCell<Option<Instant>>,
    }

    impl WorkerHandle {
        pub fn send(&self, cmd: Cmd) {
            match cmd {
                Cmd::Connect(port) if is_server_address(&port) => {
                    *self.session.borrow_mut() = None;
                    match url_for(&port) {
                        Ok(url) => {
                            let notify = self.notify.clone();
                            let open: Opener = Box::new(move |url| {
                                let notify = notify.clone();
                                open_ws(url, move || notify())
                            });
                            let hint = https_hint(&url);
                            let s = Session::start(port, url, open, self.options.remote, hint, Instant::now());
                            *self.session.borrow_mut() = Some(s);
                        }
                        Err(why) => self.own.borrow_mut().push_back(cannot_connect(&port, why)),
                    }
                }
                Cmd::Connect(port) => self.own.borrow_mut().push_back(cannot_connect(
                    &port,
                    "in the browser vna-r connects to a vna-server only (an address such as vna.local:8080)",
                )),
                cmd => match &mut *self.session.borrow_mut() {
                    Some(s) => s.command(cmd),
                    None => self.own.borrow_mut().extend(not_connected(&cmd)),
                },
            }
            (self.notify)();
        }

        pub fn try_recv(&self) -> Option<Event> {
            if let Some(e) = self.own.borrow_mut().pop_front() {
                return Some(e);
            }
            let mut session = self.session.borrow_mut();
            let s = session.as_mut()?;
            s.poll(Instant::now());
            let e = s.next_event();
            self.wake_at(s.deadline());
            e
        }

        /// A browser timer that asks for a frame at `deadline`, so that a
        /// retry happens even when nothing else moves.
        fn wake_at(&self, deadline: Option<Instant>) {
            use wasm_bindgen::closure::Closure;
            use wasm_bindgen::JsCast as _;
            let Some(d) = deadline else { return };
            if *self.timer_for.borrow() == Some(d) {
                return;
            }
            *self.timer_for.borrow_mut() = Some(d);
            let ms = d.saturating_duration_since(Instant::now()).as_millis().min(i32::MAX as u128) as i32 + 1;
            let notify = self.notify.clone();
            let callback = Closure::once_into_js(move || notify());
            if let Some(w) = web_sys::window() {
                let _ = w.set_timeout_with_callback_and_timeout_and_arguments_0(callback.unchecked_ref(), ms);
            }
        }
    }

    /// A page that came over https may open `ws://` to the local network in
    /// Chromium browsers only, after a permission prompt; elsewhere the
    /// browser refuses and says no more than that it failed (docs/10).
    fn https_hint(url: &str) -> Option<String> {
        let https = web_sys::window()
            .and_then(|w| w.location().protocol().ok())
            .is_some_and(|p| p == "https:");
        (https && url.starts_with("ws://")).then(|| {
            ". This page came over https, and browsers let such a page open an unencrypted ws:// connection \
             to a device on the local network only in Chrome or Edge, after you allow it. Open the page the \
             server itself serves (http://…) instead."
                .to_string()
        })
    }
}
