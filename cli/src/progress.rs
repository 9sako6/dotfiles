use std::io::{self, Write};
use std::sync::mpsc::{self, Sender};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

pub struct Progress {
    message: &'static str,
    started: Instant,
    stop: Sender<()>,
    worker: Option<JoinHandle<()>>,
}

impl Progress {
    pub fn start(message: &'static str) -> Self {
        let started = Instant::now();
        let _ = writeln!(io::stderr(), "dotfiles: {message}...");
        let (stop, receive) = mpsc::channel();
        let worker = thread::spawn(move || {
            while receive.recv_timeout(Duration::from_secs(10))
                == Err(mpsc::RecvTimeoutError::Timeout)
            {
                let _ = writeln!(
                    io::stderr(),
                    "dotfiles: {message}... ({}s)",
                    started.elapsed().as_secs()
                );
            }
        });
        Self {
            message,
            started,
            stop,
            worker: Some(worker),
        }
    }
}

impl Drop for Progress {
    fn drop(&mut self) {
        let _ = self.stop.send(());
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
        let _ = writeln!(
            io::stderr(),
            "dotfiles: {} ({:.1}s)",
            self.message,
            self.started.elapsed().as_secs_f64()
        );
    }
}
