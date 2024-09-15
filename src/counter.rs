use std::sync::{Arc, Mutex};

use tokio::sync::broadcast::{Receiver, Sender};

#[derive(Clone, Debug)]
pub struct Counter {
    value: Arc<Mutex<i32>>,
    sender: Sender<i32>,
}

impl Default for Counter {
    fn default() -> Self {
        Self {
            value: Default::default(),
            sender: Sender::new(2),
        }
    }
}

impl Counter {
    pub fn value(&self) -> i32 {
        *self.value.lock().unwrap()
    }

    pub fn increment(&self) {
        let value = {
            let mut counter = self.value.lock().unwrap();
            *counter += 1;
            *counter
        };

        let _ = self.sender.send(value);
    }

    pub fn decrement(&self) {
        let value = {
            let mut counter = self.value.lock().unwrap();
            *counter -= 1;
            *counter
        };
        let _ = self.sender.send(value);
    }

    pub fn subscribe(&self) -> Receiver<i32> {
        self.sender.subscribe()
    }
}
