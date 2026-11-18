/*
*   Copyright (c) 2026. caoccao.com Sam Cao
*   All rights reserved.

*   Licensed under the Apache License, Version 2.0 (the "License");
*   you may not use this file except in compliance with the License.
*   You may obtain a copy of the License at

*   http://www.apache.org/licenses/LICENSE-2.0

*   Unless required by applicable law or agreed to in writing, software
*   distributed under the License is distributed on an "AS IS" BASIS,
*   WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
*   See the License for the specific language governing permissions and
*   limitations under the License.
*/

//! The link to `bme_core::extract`: extract events in, enqueue and cancel
//! out; nothing polls.
//!
//! The core's worker threads push [`ExtractEvent`]s into an unbounded
//! channel; one foreground task applies them to the queue store in the order
//! they were sent. [`ExtractBackend`] is the other direction: enqueue and
//! cancel.

use std::sync::Arc;
use std::time::{Duration, SystemTime};

use anyhow::Result;
use async_channel::Receiver;
use bme_core::extract::{ExtractEvent, ExtractEventSink, ExtractQueue};
use gpui_kit::App;

use crate::state::Stores;

/// A sink for the core's events and the receiver [`start`] reads them from.
pub fn channel() -> (ExtractEventSink, Receiver<ExtractEvent>) {
  let (sender, receiver) = async_channel::unbounded();
  let sink: ExtractEventSink = Box::new(move |event| {
    // Fails only once the receiver is gone, when the app is quitting.
    let _ = sender.try_send(event);
  });
  (sink, receiver)
}

type EnqueueFn = dyn Fn(String, String, Vec<String>) -> Result<()> + Send + Sync;
type CancelFn = dyn Fn(&str) -> Result<()> + Send + Sync;
type CancelManyFn = dyn Fn(&[String]) -> Result<()> + Send + Sync;
type ShutdownFn = dyn Fn(Duration) -> bool + Send + Sync;

/// How long quitting waits for the cancelled extractions to end, so no
/// `mkvextract` outlives the app.
pub const SHUTDOWN_TIMEOUT: Duration = Duration::from_secs(3);

/// Enqueues and cancels extractions: the process-wide `mkvextract` queue in
/// the app, a queue with a fake launcher (or a recorder) in tests.
#[derive(Clone)]
pub struct ExtractBackend {
  enqueue: Arc<EnqueueFn>,
  cancel: Arc<CancelFn>,
  cancel_many: Arc<CancelManyFn>,
  shutdown: Arc<ShutdownFn>,
}

impl ExtractBackend {
  /// A backend from its enqueue and cancel; it cancels several files one by
  /// one (see [`Self::with_cancel_many`]) and its shutdown has nothing to
  /// wait for (see [`Self::with_shutdown`]).
  pub fn new(
    enqueue: impl Fn(String, String, Vec<String>) -> Result<()> + Send + Sync + 'static,
    cancel: impl Fn(&str) -> Result<()> + Send + Sync + 'static,
  ) -> Self {
    let cancel: Arc<CancelFn> = Arc::new(cancel);
    let each = cancel.clone();
    Self {
      enqueue: Arc::new(enqueue),
      cancel,
      cancel_many: Arc::new(move |files| files.iter().try_for_each(|file| each(file))),
      shutdown: Arc::new(|_| true),
    }
  }

  /// The same backend with `cancel_many` behind [`Self::cancel_many`].
  pub fn with_cancel_many(self, cancel_many: impl Fn(&[String]) -> Result<()> + Send + Sync + 'static) -> Self {
    Self {
      cancel_many: Arc::new(cancel_many),
      ..self
    }
  }

  /// The same backend with `shutdown` behind [`Self::shutdown`].
  pub fn with_shutdown(self, shutdown: impl Fn(Duration) -> bool + Send + Sync + 'static) -> Self {
    Self {
      shutdown: Arc::new(shutdown),
      ..self
    }
  }

  /// The process-wide queue, which runs `mkvextract`; `sink` receives its
  /// events (only the first call installs one).
  pub fn system(sink: ExtractEventSink) -> Self {
    bme_core::extract::init(sink);
    Self::new(bme_core::extract::enqueue, |file| {
      bme_core::extract::cancel(file.to_string())
    })
    .with_cancel_many(bme_core::extract::cancel_many)
    .with_shutdown(bme_core::extract::shutdown)
  }

  /// A queue of its own, e.g. with a fake launcher.
  pub fn queue(queue: Arc<ExtractQueue>) -> Self {
    let cancel_queue = queue.clone();
    let cancel_many_queue = queue.clone();
    let shutdown_queue = queue.clone();
    Self::new(
      move |file, tools, args| queue.enqueue(file, tools, args),
      move |file| cancel_queue.cancel(file),
    )
    .with_cancel_many(move |files| cancel_many_queue.cancel_many(files))
    .with_shutdown(move |timeout| shutdown_queue.shutdown(timeout))
  }

  /// Queues `file` with its `mkvextract` arguments, to run with the
  /// MKVToolNix directory `mkv_toolnix_path`.
  pub fn enqueue(&self, file: String, mkv_toolnix_path: String, args: Vec<String>) -> Result<()> {
    (self.enqueue)(file, mkv_toolnix_path, args)
  }

  /// Cancels `file`, waiting or running.
  pub fn cancel(&self, file: &str) -> Result<()> {
    (self.cancel)(file)
  }

  /// Cancels `files` at once, so a running one that ends meanwhile starts
  /// none of the others.
  pub fn cancel_many(&self, files: &[String]) -> Result<()> {
    (self.cancel_many)(files)
  }

  /// Cancels every extraction and waits up to `timeout` for the running ones
  /// to end; `false` when some were still running. Called as the app quits.
  pub fn shutdown(&self, timeout: Duration) -> bool {
    (self.shutdown)(timeout)
  }
}

/// Applies `events` to the queue store on the foreground, in order, and
/// starts what a finished extraction tells the user
/// ([`crate::notifications`]).
pub fn start(stores: &Stores, events: Receiver<ExtractEvent>, cx: &mut App) {
  crate::notifications::watch(stores, cx);
  let queue = stores.queue.clone();
  cx.spawn(async move |cx| {
    while let Ok(event) = events.recv().await {
      queue.update(cx, |queue, cx| queue.apply_event(&event, SystemTime::now(), cx));
    }
  })
  .detach();
}
