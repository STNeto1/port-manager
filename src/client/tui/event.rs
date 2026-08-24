use std::time::Duration;

use crossterm::event::{Event as CrosstermEvent, EventStream, KeyEvent};
use futures_util::StreamExt;
use tokio::sync::mpsc;

use crate::model::TunnelEvent;

#[derive(Debug, Clone)]
pub enum AppEvent {
    Tick,
    Key(KeyEvent),
    Resize(u16, u16),
    Tunnel(TunnelEvent),
}

pub struct EventHandler {
    pub receiver: mpsc::UnboundedReceiver<AppEvent>,
    /// Cloned by the daemon event-subscription task to inject `Tunnel`
    /// events into the same stream as Tick/Key/Resize.
    pub sender: mpsc::UnboundedSender<AppEvent>,
}

impl EventHandler {
    pub fn new(tick_rate: Duration) -> Self {
        let (sender, receiver) = mpsc::unbounded_channel();

        let tick_sender = sender.clone();
        tokio::spawn(async move {
            let mut interval = tokio::time::interval(tick_rate);
            loop {
                interval.tick().await;
                if tick_sender.send(AppEvent::Tick).is_err() {
                    break;
                }
            }
        });

        let key_sender = sender.clone();
        tokio::spawn(async move {
            let mut stream = EventStream::new();
            while let Some(Ok(event)) = stream.next().await {
                let app_event = match event {
                    CrosstermEvent::Key(key) => Some(AppEvent::Key(key)),
                    CrosstermEvent::Resize(width, height) => Some(AppEvent::Resize(width, height)),
                    _ => None,
                };
                if let Some(app_event) = app_event
                    && key_sender.send(app_event).is_err()
                {
                    break;
                }
            }
        });

        Self { receiver, sender }
    }
}
