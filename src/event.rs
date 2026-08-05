use std::sync::mpsc;
use std::time::Duration;

use crossterm::event::{self, Event as CtEvent, KeyEventKind};

/// A raw input/timer event, translated from crossterm and the tick interval. `App` turns these
/// into `Action`s.
#[derive(Debug, Clone, Copy)]
pub enum Event {
    Key(crossterm::event::KeyEvent),
    Resize,
    Tick,
}

/// Polls crossterm for terminal events on a background thread, interleaving a fixed-rate tick so
/// the timer redraws once a second even with no key input. This is the synchronous analogue of
/// gitv's tokio event-pump task: no async runtime is needed since the only periodic work here is
/// a plain sleep/poll loop.
///
/// The thread is not explicitly joined on shutdown: `crossterm::event::poll` blocks for up to
/// `tick_rate`, so the thread would otherwise have to be woken to observe a cancellation flag.
/// Since it does nothing but forward events and holds no resources needing cleanup, letting the
/// process exit reap it is the simpler and standard choice for this kind of poll loop.
pub struct EventHandler {
    rx: mpsc::Receiver<Event>,
}

impl EventHandler {
    pub fn new(tick_rate: Duration) -> Self {
        let (tx, rx) = mpsc::channel();
        std::thread::spawn(move || Self::run(tx, tick_rate));
        Self { rx }
    }

    fn run(tx: mpsc::Sender<Event>, tick_rate: Duration) {
        loop {
            let event = if event::poll(tick_rate).unwrap_or(false) {
                match event::read() {
                    Ok(CtEvent::Key(key)) if key.kind != KeyEventKind::Release => {
                        Some(Event::Key(key))
                    }
                    Ok(CtEvent::Resize(..)) => Some(Event::Resize),
                    _ => None,
                }
            } else {
                Some(Event::Tick)
            };
            let Some(event) = event else { continue };
            if tx.send(event).is_err() {
                return;
            }
        }
    }

    pub fn next(&self) -> Option<Event> {
        self.rx.recv().ok()
    }
}
