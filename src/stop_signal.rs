use tokio::sync::broadcast::{Receiver, Sender};

#[derive(Clone, Debug)]
pub struct StopSignal(Sender<()>);

impl StopSignal {
    pub fn subscribe(&self) -> Receiver<()> {
        self.0.subscribe()
    }

    pub fn send_stop(&self) {
        let _ = self.0.send(());
    }
}

impl Default for StopSignal {
    fn default() -> Self {
        Self(Sender::new(1))
    }
}
