use futures_channel::oneshot;
use std::{
    sync::{
        Arc, OnceLock,
        atomic::{AtomicBool, Ordering},
        mpsc,
    },
    thread,
    time::{Duration, Instant},
};
use windows::Win32::System::WinRT::{RO_INIT_MULTITHREADED, RoInitialize, RoUninitialize};

pub(super) struct Cancellation {
    cancelled: AtomicBool,
    deadline: Instant,
}

impl Cancellation {
    fn new(timeout: Duration) -> Self {
        Self {
            cancelled: AtomicBool::new(false),
            deadline: Instant::now() + timeout,
        }
    }

    pub fn check(&self) -> Result<(), String> {
        if self.cancelled.load(Ordering::Acquire) {
            Err("Screen capture was cancelled.".into())
        } else if Instant::now() >= self.deadline {
            Err("Screen capture took too long. Please try again.".into())
        } else {
            Ok(())
        }
    }
}

struct CancelOnDrop(Arc<Cancellation>);

impl Drop for CancelOnDrop {
    fn drop(&mut self) {
        self.0.cancelled.store(true, Ordering::Release);
    }
}

struct Apartment;

impl Drop for Apartment {
    fn drop(&mut self) {
        unsafe { RoUninitialize() };
    }
}

type Job = Box<dyn FnOnce(Result<(), String>) + Send>;

fn capture_worker() -> Result<&'static mpsc::Sender<Job>, String> {
    static WORKER: OnceLock<Result<mpsc::Sender<Job>, String>> = OnceLock::new();
    WORKER
        .get_or_init(|| {
            let (sender, receiver) = mpsc::channel::<Job>();
            thread::Builder::new()
                .name("screenshot".into())
                .spawn(move || {
                    let initialized =
                        unsafe { RoInitialize(RO_INIT_MULTITHREADED) }.map_err(super::error);
                    let _apartment = initialized.is_ok().then_some(Apartment);
                    // WGC can finish internal work after Close. Keep its apartment alive across jobs.
                    for job in receiver {
                        job(initialized.clone());
                    }
                })
                .map_err(|error| format!("Unable to start screen capture: {error}"))?;
            Ok(sender)
        })
        .as_ref()
        .map_err(Clone::clone)
}

pub(super) async fn run<T: Send + 'static>(
    work: impl FnOnce(&Cancellation) -> Result<T, String> + Send + 'static,
) -> Result<T, String> {
    let cancel = Arc::new(Cancellation::new(Duration::from_secs(15)));
    let _guard = CancelOnDrop(cancel.clone());
    let (sender, receiver) = oneshot::channel();
    capture_worker()?
        .send(Box::new(move |initialized| {
            let result = (|| {
                initialized?;
                cancel.check()?;
                let value = work(&cancel)?;
                cancel.check()?;
                Ok(value)
            })();
            let _ = sender.send(result);
        }))
        .map_err(|_| "The capture worker stopped unexpectedly.".to_string())?;
    receiver
        .await
        .map_err(|_| "The capture worker stopped unexpectedly.".to_string())?
}
