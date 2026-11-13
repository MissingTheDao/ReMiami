//! An optional render thread: the back end and the presenter move to their own thread, the
//! front end stays with the caller. Frames go over a channel with room for one, so the caller
//! records frame n + 1 while frame n is drawn and is at most one frame ahead. Drawn frames come
//! back for reuse. A frame that asks for a capture is waited for, so screenshots are synchronous
//! as before.

use crate::frame::{Frame, WgpuBackend};
use crate::present::Presenter;
use std::sync::mpsc::{Receiver, SyncSender, TryRecvError, sync_channel};
use std::thread::JoinHandle;

/// An RGBA8 screenshot: width, height, pixels (top row first).
pub type Shot = (u32, u32, Vec<u8>);

struct Job {
    frame: Frame,
    /// Output size the window has now (the presenter resizes when it differs).
    size: (u32, u32),
    vsync: bool,
    capture: bool,
}

/// A frame that was drawn, back from the render thread.
pub struct Done {
    pub frame: Frame,
    pub shot: Option<Shot>,
    pub error: Option<String>,
    /// Whether the frame asked for a capture.
    pub capture: bool,
}

pub struct RenderThread {
    jobs: Option<SyncSender<Job>>,
    done: Receiver<Done>,
    handle: Option<JoinHandle<()>>,
}

impl RenderThread {
    /// Moves `presenter` and `back` to a new thread named `revc-render`.
    pub fn spawn(mut presenter: Presenter, mut back: WgpuBackend) -> Result<RenderThread, String> {
        let (jobs, rx) = sync_channel::<Job>(1);
        let (tx, done) = sync_channel::<Done>(4);
        let handle = std::thread::Builder::new()
            .name("revc-render".into())
            .spawn(move || {
                for mut job in rx {
                    if presenter.size() != job.size {
                        presenter.resize(job.size.0, job.size.1);
                    }
                    presenter.set_vsync(job.vsync);
                    let (shot, error) = match presenter.present(&mut back, &mut job.frame, job.capture) {
                        Ok(s) => (s, None),
                        Err(e) => (None, Some(e)),
                    };
                    if tx.send(Done { frame: job.frame, shot, error, capture: job.capture }).is_err() {
                        break;
                    }
                }
            })
            .map_err(|e| format!("render thread: {e}"))?;
        Ok(RenderThread { jobs: Some(jobs), done, handle: Some(handle) })
    }

    /// Hands `frame` to the render thread (blocks while one frame is still queued). Returns the
    /// frames drawn since the last call; with `capture`, waits until this frame is drawn, so the
    /// last returned `Done` carries its screenshot.
    pub fn submit(&mut self, frame: Frame, size: (u32, u32), vsync: bool, capture: bool) -> Vec<Done> {
        let mut out = Vec::new();
        let Some(jobs) = &self.jobs else { return out };
        if jobs.send(Job { frame, size, vsync, capture }).is_err() {
            return out;
        }
        if capture {
            while let Ok(d) = self.done.recv() {
                let last = d.capture;
                out.push(d);
                if last {
                    break;
                }
            }
        } else {
            loop {
                match self.done.try_recv() {
                    Ok(d) => out.push(d),
                    Err(TryRecvError::Empty | TryRecvError::Disconnected) => break,
                }
            }
        }
        out
    }

    /// Waits until every submitted frame is drawn and returns them.
    pub fn finish(&mut self) -> Vec<Done> {
        let mut out = Vec::new();
        if let Some(jobs) = self.jobs.take() {
            drop(jobs);
            out.extend(self.done.iter());
        }
        if let Some(h) = self.handle.take() {
            let _ = h.join();
        }
        out
    }
}

impl Drop for RenderThread {
    fn drop(&mut self) {
        self.finish();
    }
}
