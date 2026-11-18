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

//! The extract queue as the UI shows it: one [`QueueItem`] per enqueued
//! file, in the order it was first enqueued, kept current by
//! `bme_core::extract`'s events (see [`crate::extract_events`]).
//!
//! A change notifies observers only when it is structural (an item added or
//! removed, or a status changed): the toolbar, the title and the close guard
//! care about that. Every change, progress included, is also emitted as
//! [`QueueEvent::ItemChanged`] naming the file, so a card or a queue row can
//! repaint alone.
//!
//! A batch runs from the moment an item waits or extracts until none does; its
//! end is emitted once as [`QueueEvent::Drained`], which sends the completion
//! notifications once per batch, not per finished file.
//!
//! A file removed while the extract queue still has it stays, hidden, until
//! its `Finished` arrives, which then drops it: the item counts as active
//! until the extraction really ended, and the late `Cancelled` lists nothing
//! again.
//!
//! An extraction is listed as waiting from the moment it is asked for, while
//! it is prepared on the background executor ([`QueueStore::reserve`]):
//! Cancel, Delete and Cancel All see it, and a cancelled reservation ends at
//! once, as the extract queue ends a waiting file. Once handed to the extract
//! queue ([`QueueStore::hand_over`]), the files it writes are its own until it
//! finishes: no other extraction may write one of them
//! ([`QueueStore::output_owner`]).

use std::collections::{HashMap, HashSet};
use std::time::{Duration, SystemTime};

use bme_core::extract::{ExtractError, ExtractEvent, ExtractOutcome};
use gpui_kit::{Context, EventEmitter, Task};

use crate::domain::status::{QueueItem, QueueItemStatus, can_enqueue};

/// How often the elapsed and ETA labels move on while an extraction runs: the
/// one-second resolution of `format_hms`.
pub const TICK: Duration = Duration::from_secs(1);

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum QueueEvent {
  /// The item of `file` changed (its progress only, too) or was removed.
  ItemChanged { file: String },
  /// An extraction ended; `elapsed` runs from its start to its end (zero
  /// when it never started). `warnings` are what `mkvextract` warned about.
  Finished {
    file: String,
    outcome: ExtractOutcome,
    error: Option<ExtractError>,
    warnings: Vec<String>,
    elapsed: Duration,
  },
  /// A second passed while at least one extraction runs.
  Tick,
  /// The last waiting or extracting item finished or went: the batch is over.
  Drained(QueueBatch),
}

/// What one batch of extractions ended with: every extraction that finished between
/// the queue becoming active and the last active item going.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct QueueBatch {
  pub completed: usize,
  pub failed: usize,
  pub cancelled: usize,
  /// The batch's first start and its last end.
  pub started: Option<SystemTime>,
  pub ended: Option<SystemTime>,
  /// The file that finished last, with its own elapsed time.
  pub last: Option<(String, Duration)>,
}

impl QueueBatch {
  /// Extractions that finished in the batch.
  pub fn finished(&self) -> usize {
    self.completed + self.failed + self.cancelled
  }

  /// Whether the batch is worth a notification: an extraction completed or
  /// failed. A batch the user cancelled entirely is not.
  pub fn notifies(&self) -> bool {
    self.completed + self.failed > 0
  }

  /// From the first start to the last end; zero without either.
  pub fn elapsed(&self) -> Duration {
    match (self.started, self.ended) {
      (Some(started), Some(ended)) => ended.duration_since(started).unwrap_or_default(),
      _ => Duration::ZERO,
    }
  }

  fn start(&mut self, at: SystemTime) {
    self.started = Some(self.started.map_or(at, |started| started.min(at)));
  }

  fn finish(&mut self, file: &str, outcome: ExtractOutcome, at: SystemTime, elapsed: Duration) {
    match outcome {
      ExtractOutcome::Completed => self.completed += 1,
      ExtractOutcome::Failed => self.failed += 1,
      ExtractOutcome::Cancelled => self.cancelled += 1,
    }
    self.ended = Some(self.ended.map_or(at, |ended| ended.max(at)));
    self.last = Some((file.to_string(), elapsed));
  }
}

/// The queue in numbers, for the window title and the taskbar progress.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct QueueSummary {
  /// Completed, cancelled or failed items.
  pub done: usize,
  pub total: usize,
  /// Whether an item is waiting or extracting.
  pub active: bool,
  /// The whole queue's progress, 0–100: a finished item counts 100, an extracting
  /// one its progress, a waiting one 0; the mean is rounded down.
  pub percent: u32,
}

impl QueueSummary {
  /// The summary counted over `items` (tests check the kept totals by it).
  #[cfg(test)]
  fn of<'a>(items: impl IntoIterator<Item = &'a QueueItem>) -> Self {
    let mut summary = Self::default();
    let mut progress_sum: u64 = 0;
    for item in items {
      summary.total += 1;
      match item.status {
        QueueItemStatus::Waiting => summary.active = true,
        QueueItemStatus::Extracting => {
          summary.active = true;
          progress_sum += u64::from(item.progress);
        }
        QueueItemStatus::Completed | QueueItemStatus::Cancelled | QueueItemStatus::Failed => {
          summary.done += 1;
          progress_sum += 100;
        }
      }
    }
    if summary.total > 0 {
      summary.percent = (progress_sum / summary.total as u64).min(100) as u32;
    }
    summary
  }
}

/// Running counts of the queue, kept as items change, so a progress event
/// or a tick costs no scan of the whole queue.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
struct Totals {
  /// Shown items (a removed file whose extraction is ending is hidden).
  shown: usize,
  /// Shown completed, cancelled or failed items.
  done: usize,
  /// Shown waiting or extracting items.
  shown_active: usize,
  /// The shown items' progress: 100 for a finished one, its percentage for
  /// an extracting one.
  progress: u64,
  /// Waiting or extracting items, hidden ones included.
  active: usize,
  /// Extracting items, hidden ones included.
  extracting: usize,
}

impl Totals {
  /// What `item` adds, `shown` or hidden.
  fn of(item: Option<&QueueItem>, shown: bool) -> Self {
    let Some(item) = item else {
      return Self::default();
    };
    let extracting = item.status == QueueItemStatus::Extracting;
    let mut totals = Self {
      active: usize::from(item.status.is_active()),
      extracting: usize::from(extracting),
      ..Self::default()
    };
    if shown {
      totals.shown = 1;
      if item.status.is_terminal() {
        totals.done = 1;
        totals.progress = 100;
      } else {
        totals.shown_active = 1;
        if extracting {
          totals.progress = u64::from(item.progress);
        }
      }
    }
    totals
  }

  fn add(&mut self, other: Self) {
    self.shown += other.shown;
    self.done += other.done;
    self.shown_active += other.shown_active;
    self.progress += other.progress;
    self.active += other.active;
    self.extracting += other.extracting;
  }

  fn sub(&mut self, other: Self) {
    self.shown -= other.shown;
    self.done -= other.done;
    self.shown_active -= other.shown_active;
    self.progress -= other.progress;
    self.active -= other.active;
    self.extracting -= other.extracting;
  }

  fn summary(&self) -> QueueSummary {
    QueueSummary {
      done: self.done,
      total: self.shown,
      active: self.shown_active > 0,
      percent: if self.shown > 0 {
        (self.progress / self.shown as u64).min(100) as u32
      } else {
        0
      },
    }
  }
}

#[derive(Default)]
pub struct QueueStore {
  items: HashMap<String, QueueItem>,
  order: Vec<String>,
  /// Files removed while the extract queue had them: hidden, and dropped by
  /// their `Finished`.
  discarded: HashSet<String>,
  /// Runs while an item is extracting and emits [`QueueEvent::Tick`].
  ticker: Option<Task<()>>,
  /// The batch running while an item is waiting or extracting.
  batch: Option<QueueBatch>,
  /// Files listed as waiting while their extraction is prepared, with the
  /// token of their reservation.
  preparing: HashMap<String, u64>,
  /// The last reservation's token.
  last_reservation: u64,
  /// The [`output_key`](bme_core::extract_args::output_key)s of the files
  /// each handed-over extraction writes, until it finishes.
  outputs: HashMap<String, Vec<String>>,
  /// Which extraction writes each of those keys.
  output_owners: HashMap<String, String>,
  /// The counts behind [`Self::summary`], [`Self::has_active`], the ticker
  /// and the batch.
  totals: Totals,
}

impl EventEmitter<QueueEvent> for QueueStore {}

impl QueueStore {
  pub fn new() -> Self {
    Self::default()
  }

  /// The item of `file`; `None` too while a removed file's extraction is
  /// ending.
  pub fn item(&self, file: &str) -> Option<&QueueItem> {
    self.items.get(file).filter(|_| self.shown(file))
  }

  /// The items in queue order.
  pub fn items(&self) -> impl Iterator<Item = &QueueItem> {
    self
      .order
      .iter()
      .filter(|file| self.shown(file))
      .filter_map(|file| self.items.get(file))
  }

  /// Cancelled and failed items in queue order.
  pub fn resumable_items(&self) -> impl Iterator<Item = &QueueItem> {
    self.items().filter(|item| item.status.is_resumable())
  }

  /// Whether `file` may be enqueued: it is not waiting or extracting, nor a
  /// removed file whose extraction is still ending.
  pub fn can_enqueue(&self, file: &str) -> bool {
    can_enqueue(self.items.get(file))
  }

  /// Whether an extraction is waiting or running, a removed file's included.
  pub fn has_active(&self) -> bool {
    self.totals.active > 0
  }

  /// The waiting and extracting files in queue order, removed files whose
  /// extraction is still ending included.
  pub fn active_files(&self) -> Vec<String> {
    self
      .order
      .iter()
      .filter_map(|file| self.items.get(file))
      .filter(|item| item.status.is_active())
      .map(|item| item.file.clone())
      .collect()
  }

  pub fn summary(&self) -> QueueSummary {
    self.totals.summary()
  }

  /// What the item of `file` adds to the totals now.
  fn tally(&self, file: &str) -> Totals {
    Totals::of(self.items.get(file), self.shown(file))
  }

  /// Runs `edit`, which changes the item of `file` only, and keeps the
  /// totals.
  fn edit<R>(&mut self, file: &str, edit: impl FnOnce(&mut Self) -> R) -> R {
    let before = self.tally(file);
    let result = edit(self);
    let after = self.tally(file);
    self.totals.sub(before);
    self.totals.add(after);
    result
  }

  fn shown(&self, file: &str) -> bool {
    !self.discarded.contains(file)
  }

  /// The drives with their files, each in queue order; the drives in the order
  /// their first file was enqueued.
  pub fn drives(&self) -> Vec<(String, Vec<String>)> {
    let mut drives: Vec<(String, Vec<String>)> = Vec::new();
    for item in self.items() {
      match drives.iter_mut().find(|(drive, _)| *drive == item.drive) {
        Some((_, files)) => files.push(item.file.clone()),
        None => drives.push((item.drive.clone(), vec![item.file.clone()])),
      }
    }
    drives
  }

  /// The items of one drive, in queue order.
  pub fn items_in_drive<'a>(&'a self, drive: &'a str) -> impl Iterator<Item = &'a QueueItem> + 'a {
    self.items().filter(move |item| item.drive == drive)
  }

  /// Lists `file` as waiting, as it is handed to the extract queue. A file
  /// that is waiting or extracting already is left alone (returns `false`); a
  /// finished one starts over in its old position.
  pub fn add_to_queue(&mut self, file: &str, cx: &mut Context<Self>) -> bool {
    if !self.can_enqueue(file) {
      return false;
    }
    let added = self.edit(file, |this| {
      this.items.insert(file.to_string(), QueueItem::waiting(file)).is_none()
    });
    if added {
      self.order.push(file.to_string());
    }
    self.changed(file, true, cx);
    true
  }

  /// Lists `file` as waiting while its extraction is prepared, before the
  /// extract queue has it, as [`Self::add_to_queue`] does. Returns the token
  /// of the reservation, which [`Self::is_reserved`] checks before the hand
  /// over; `None` when the file is waiting or extracting already.
  pub fn reserve(&mut self, file: &str, cx: &mut Context<Self>) -> Option<u64> {
    if !self.add_to_queue(file, cx) {
      return None;
    }
    self.last_reservation += 1;
    self.preparing.insert(file.to_string(), self.last_reservation);
    Some(self.last_reservation)
  }

  /// Whether the reservation `token` of `file` stands: it was not
  /// cancelled, removed or handed over since.
  pub fn is_reserved(&self, file: &str, token: u64) -> bool {
    self.preparing.get(file) == Some(&token)
  }

  /// The other waiting or extracting file that writes one of `outputs`
  /// (output keys), with the index of that output.
  pub fn output_owner(&self, file: &str, outputs: &[String]) -> Option<(usize, &str)> {
    outputs.iter().enumerate().find_map(|(index, output)| {
      self
        .output_owners
        .get(output)
        .filter(|owner| *owner != file)
        .map(|owner| (index, owner.as_str()))
    })
  }

  /// The reserved `file` goes to the extract queue: from now on it writes
  /// `outputs` (output keys) until it finishes.
  pub fn hand_over(&mut self, file: &str, outputs: Vec<String>) {
    self.preparing.remove(file);
    self.release_outputs(file);
    for output in &outputs {
      self.output_owners.insert(output.clone(), file.to_string());
    }
    self.outputs.insert(file.to_string(), outputs);
  }

  /// Cancels the reservation of `file` before the extract queue has it: the
  /// item finishes as cancelled at once. `false` when `file` is not being
  /// prepared.
  pub fn cancel_reservation(&mut self, file: &str, now: SystemTime, cx: &mut Context<Self>) -> bool {
    if self.preparing.remove(file).is_none() {
      return false;
    }
    let cancelled = ExtractEvent::Finished {
      file: file.to_string(),
      outcome: ExtractOutcome::Cancelled,
      error: None,
      warnings: Vec::new(),
      at: now,
    };
    self.apply_event(&cancelled, now, cx);
    true
  }

  /// Takes `file` off the queue: its preparation failed, or the extract
  /// queue refused it.
  pub fn abandon(&mut self, file: &str, cx: &mut Context<Self>) {
    self.drop_item(file, cx);
  }

  /// Drops the item of `file`. While the extract queue still has it, the item
  /// is only hidden: it counts as active until its `Finished` arrives,
  /// which drops it without a row, a toast or a place in the batch. A file
  /// still being prepared is not in the extract queue and goes at once.
  pub fn remove_from_queue(&mut self, file: &str, cx: &mut Context<Self>) {
    let Some(item) = self.items.get(file) else {
      return;
    };
    if !item.status.is_active() || self.preparing.contains_key(file) {
      self.drop_item(file, cx);
    } else if self.edit(file, |this| this.discarded.insert(file.to_string())) {
      self.changed(file, true, cx);
    }
  }

  fn drop_item(&mut self, file: &str, cx: &mut Context<Self>) {
    if self.forget_item(file) {
      self.order.retain(|listed| listed != file);
      self.changed(file, true, cx);
    }
  }

  /// Drops everything about the item of `file` but its place in `order`;
  /// whether there was an item.
  fn forget_item(&mut self, file: &str) -> bool {
    self.preparing.remove(file);
    self.release_outputs(file);
    self.edit(file, |this| {
      this.discarded.remove(file);
      this.items.remove(file).is_some()
    })
  }

  /// The outputs of `file` are free again.
  fn release_outputs(&mut self, file: &str) {
    for output in self.outputs.remove(file).unwrap_or_default() {
      if self.output_owners.get(&output).is_some_and(|owner| owner == file) {
        self.output_owners.remove(&output);
      }
    }
  }

  /// Flags a cancel request on a waiting or extracting item; the outcome
  /// still comes from the extract events.
  pub fn mark_cancel_requested(&mut self, file: &str, cx: &mut Context<Self>) {
    if self.items.get_mut(file).is_some_and(QueueItem::request_cancel) {
      self.changed(file, false, cx);
    }
  }

  /// A cancel of `file` could not stop its `mkvextract`: the request is
  /// withdrawn, so Cancel can be tried again.
  pub fn cancel_failed(&mut self, file: &str, cx: &mut Context<Self>) {
    if let Some(item) = self.items.get_mut(file)
      && item.cancel_requested
    {
      item.cancel_requested = false;
      self.changed(file, false, cx);
    }
  }

  /// Applies one extract event. An event for a file that is not listed lists
  /// it.
  /// `now` stands in for a time the event does not carry.
  pub fn apply_event(&mut self, event: &ExtractEvent, now: SystemTime, cx: &mut Context<Self>) {
    let file = event.file();
    let finished = matches!(event, ExtractEvent::Finished { .. });
    if finished {
      // Whatever it wrote is done.
      self.preparing.remove(file);
      self.release_outputs(file);
    }
    if !self.shown(file) {
      // A removed file: its extraction's end drops it.
      if finished {
        self.drop_item(file, cx);
      }
      return;
    }
    let applied = self.edit(file, |this| match this.items.get_mut(file) {
      Some(item) => {
        let status = item.status;
        item.apply(event, now).then_some((item.status != status, false))
      }
      None => {
        this.items.insert(file.to_string(), QueueItem::from_event(event, now));
        Some((true, true))
      }
    });
    let Some((structural, listed)) = applied else {
      return;
    };
    if listed {
      self.order.push(file.to_string());
    }
    if let ExtractEvent::Started { at, .. } = event {
      self.batch.get_or_insert_default().start(*at);
    }
    if let ExtractEvent::Finished {
      outcome,
      error,
      warnings,
      at,
      ..
    } = event
    {
      let elapsed = self.items[file].elapsed(*at).unwrap_or_default();
      // A finish while nothing was active (a late `Cancelled` for a removed
      // item) belongs to no batch.
      if let Some(batch) = self.batch.as_mut() {
        batch.finish(file, *outcome, *at, elapsed);
      }
      cx.emit(QueueEvent::Finished {
        file: file.to_string(),
        outcome: *outcome,
        error: error.clone(),
        warnings: warnings.clone(),
        elapsed,
      });
    }
    self.changed(file, structural, cx);
  }

  /// Removes completed, cancelled and failed items from the queue.
  pub fn clear_completed(&mut self, cx: &mut Context<Self>) {
    let removed: Vec<String> = self
      .items()
      .filter(|item| item.status.is_terminal())
      .map(|item| item.file.clone())
      .collect();
    self.forget_all(&removed, cx);
  }

  /// Drops the items of `files` with one pass over `order`.
  fn forget_all(&mut self, files: &[String], cx: &mut Context<Self>) {
    let mut dropped = HashSet::new();
    for file in files {
      if self.forget_item(file) {
        dropped.insert(file.as_str());
      }
    }
    if dropped.is_empty() {
      return;
    }
    self.order.retain(|file| !dropped.contains(file.as_str()));
    for file in dropped {
      self.changed(file, true, cx);
    }
  }

  /// Moves the item of `file` to `drive`, as if it were on another drive
  /// (drive keys come from Windows paths only).
  #[cfg(test)]
  pub(crate) fn move_to_drive(&mut self, file: &str, drive: &str, cx: &mut Context<Self>) {
    if let Some(item) = self.items.get_mut(file) {
      item.drive = drive.to_string();
      self.changed(file, true, cx);
    }
  }

  /// Removes every item (the queue part of Clear), as
  /// [`Self::remove_from_queue`] does: a file the extract queue still has is
  /// hidden, the others go, with one pass over `order`.
  pub fn clear(&mut self, cx: &mut Context<Self>) {
    let mut dropped = Vec::new();
    for file in self.order.clone() {
      let Some(item) = self.items.get(&file) else {
        continue;
      };
      if !item.status.is_active() || self.preparing.contains_key(&file) {
        dropped.push(file);
      } else if self.edit(&file, |this| this.discarded.insert(file.clone())) {
        self.changed(&file, true, cx);
      }
    }
    self.forget_all(&dropped, cx);
  }

  fn changed(&mut self, file: &str, structural: bool, cx: &mut Context<Self>) {
    #[cfg(test)]
    assert_eq!(self.totals, self.recount(), "the totals after a change of {file}");
    cx.emit(QueueEvent::ItemChanged { file: file.to_string() });
    if structural {
      cx.notify();
    }
    self.update_ticker(cx);
    self.update_batch(cx);
  }

  /// The totals counted from scratch, which the kept ones must equal.
  #[cfg(test)]
  fn recount(&self) -> Totals {
    let mut totals = Totals::default();
    for file in self.items.keys() {
      totals.add(self.tally(file));
    }
    assert_eq!(
      totals.summary(),
      QueueSummary::of(self.items()),
      "the summary as it was counted"
    );
    totals
  }

  /// Starts a batch when an item waits or extracts, and ends it (emitting
  /// [`QueueEvent::Drained`] once) when none does any more.
  fn update_batch(&mut self, cx: &mut Context<Self>) {
    if self.has_active() {
      self.batch.get_or_insert_default();
    } else if let Some(batch) = self.batch.take() {
      cx.emit(QueueEvent::Drained(batch));
    }
  }

  /// Starts the ticker when an extraction runs and stops it when none does.
  fn update_ticker(&mut self, cx: &mut Context<Self>) {
    if self.totals.extracting == 0 {
      self.ticker = None;
    } else if self.ticker.is_none() {
      self.ticker = Some(cx.spawn(async move |this, cx| {
        loop {
          cx.background_executor().timer(TICK).await;
          if this.update(cx, |_, cx| cx.emit(QueueEvent::Tick)).is_err() {
            break;
          }
        }
      }));
    }
  }
}

#[cfg(test)]
mod tests {
  use super::*;
  use core::prelude::v1::test;
  use gpui_kit::{AppContext, Entity, TestAppContext};
  use std::cell::RefCell;
  use std::rc::Rc;

  const A: &str = "/media/a.mkv";
  const B: &str = "/media/b.mkv";

  fn at(seconds: u64) -> SystemTime {
    SystemTime::UNIX_EPOCH + Duration::from_secs(1_000_000 + seconds)
  }

  fn started(file: &str, seconds: u64) -> ExtractEvent {
    ExtractEvent::Started {
      file: file.to_string(),
      at: at(seconds),
    }
  }

  fn progress(file: &str, percent: u32) -> ExtractEvent {
    ExtractEvent::Progress {
      file: file.to_string(),
      percent,
    }
  }

  fn finished(file: &str, outcome: ExtractOutcome, seconds: u64) -> ExtractEvent {
    ExtractEvent::Finished {
      file: file.to_string(),
      outcome,
      error: (outcome == ExtractOutcome::Failed).then(|| ExtractError::Exited {
        code: Some(2),
        messages: Vec::new(),
      }),
      warnings: Vec::new(),
      at: at(seconds),
    }
  }

  /// The store, the events it emitted and how often it notified.
  struct Harness {
    queue: Entity<QueueStore>,
    events: Rc<RefCell<Vec<QueueEvent>>>,
    notified: Rc<RefCell<usize>>,
    _subscriptions: Vec<gpui_kit::Subscription>,
  }

  impl Harness {
    fn new(cx: &mut TestAppContext) -> Self {
      let queue = cx.new(|_| QueueStore::new());
      let events = Rc::new(RefCell::new(Vec::new()));
      let notified = Rc::new(RefCell::new(0));
      let (event_sink, notify_sink) = (events.clone(), notified.clone());
      let subscriptions = cx.update(|cx| {
        vec![
          cx.subscribe(&queue, move |_, event: &QueueEvent, _| {
            event_sink.borrow_mut().push(event.clone())
          }),
          cx.observe(&queue, move |_, _| *notify_sink.borrow_mut() += 1),
        ]
      });
      Self {
        queue,
        events,
        notified,
        _subscriptions: subscriptions,
      }
    }

    fn apply(&self, cx: &mut TestAppContext, event: ExtractEvent) {
      self
        .queue
        .update(cx, |queue, cx| queue.apply_event(&event, at(1_000), cx));
    }

    fn status(&self, cx: &mut TestAppContext, file: &str) -> Option<QueueItemStatus> {
      self
        .queue
        .read_with(cx, |queue, _| queue.item(file).map(|item| item.status))
    }

    fn take_events(&self) -> Vec<QueueEvent> {
      std::mem::take(&mut self.events.borrow_mut())
    }

    fn take_notified(&self) -> usize {
      std::mem::take(&mut self.notified.borrow_mut())
    }
  }

  fn changed(file: &str) -> QueueEvent {
    QueueEvent::ItemChanged { file: file.to_string() }
  }

  #[gpui_kit::test]
  fn a_scripted_event_sequence_drives_each_outcome(cx: &mut TestAppContext) {
    for outcome in [
      ExtractOutcome::Completed,
      ExtractOutcome::Cancelled,
      ExtractOutcome::Failed,
    ] {
      let harness = Harness::new(cx);
      harness.queue.update(cx, |queue, cx| assert!(queue.add_to_queue(A, cx)));
      assert_eq!(harness.status(cx, A), Some(QueueItemStatus::Waiting));
      harness.apply(cx, started(A, 10));
      assert_eq!(harness.status(cx, A), Some(QueueItemStatus::Extracting));
      harness.apply(cx, progress(A, 40));
      harness.apply(cx, finished(A, outcome, 70));
      let item = harness.queue.read_with(cx, |queue, _| queue.item(A).cloned().unwrap());
      assert_eq!(item.status, QueueItemStatus::from(outcome));
      assert_eq!(
        item.progress,
        if outcome == ExtractOutcome::Completed { 100 } else { 40 }
      );
      assert_eq!(item.error.is_some(), outcome == ExtractOutcome::Failed);
      // A terminal state is never overwritten by a late progress event.
      harness.apply(cx, progress(A, 80));
      assert_eq!(harness.status(cx, A), Some(QueueItemStatus::from(outcome)));
      let events = harness.take_events();
      assert!(events.contains(&QueueEvent::Finished {
        file: A.to_string(),
        outcome,
        error: item.error.clone(),
        warnings: Vec::new(),
        elapsed: Duration::from_secs(60),
      }));
      // Waiting, extracting, finished: three structural changes; progress is not.
      assert_eq!(harness.take_notified(), 3);
    }
  }

  #[gpui_kit::test]
  fn progress_is_emitted_without_notifying(cx: &mut TestAppContext) {
    let harness = Harness::new(cx);
    harness.queue.update(cx, |queue, cx| queue.add_to_queue(A, cx));
    harness.apply(cx, started(A, 0));
    harness.take_events();
    harness.take_notified();
    harness.apply(cx, progress(A, 5));
    harness.apply(cx, progress(A, 6));
    assert_eq!(harness.take_events(), [changed(A), changed(A)]);
    assert_eq!(harness.take_notified(), 0);
  }

  #[gpui_kit::test]
  fn add_to_queue_skips_active_files_and_restarts_finished_ones(cx: &mut TestAppContext) {
    let harness = Harness::new(cx);
    harness.queue.update(cx, |queue, cx| {
      assert!(queue.add_to_queue(A, cx));
      assert!(queue.add_to_queue(B, cx));
      assert!(!queue.add_to_queue(A, cx), "already waiting");
    });
    harness.apply(cx, started(A, 0));
    harness
      .queue
      .update(cx, |queue, cx| assert!(!queue.add_to_queue(A, cx), "extracting"));
    harness.apply(cx, finished(A, ExtractOutcome::Failed, 5));
    harness.queue.update(cx, |queue, cx| {
      assert!(queue.add_to_queue(A, cx), "a failed file can be extracted again");
      let item = queue.item(A).unwrap();
      assert_eq!(item.status, QueueItemStatus::Waiting);
      assert_eq!((item.error.as_ref(), item.extract_started_at), (None, None));
      let order: Vec<&str> = queue.items().map(|item| item.file.as_str()).collect();
      assert_eq!(order, [A, B], "it keeps its position");
    });
  }

  #[gpui_kit::test]
  fn an_event_for_an_unlisted_file_lists_it(cx: &mut TestAppContext) {
    let harness = Harness::new(cx);
    harness.apply(cx, finished(B, ExtractOutcome::Cancelled, 3));
    harness.apply(cx, started(A, 4));
    harness.queue.read_with(cx, |queue, _| {
      let order: Vec<(&str, QueueItemStatus)> = queue.items().map(|item| (item.file.as_str(), item.status)).collect();
      assert_eq!(
        order,
        [(B, QueueItemStatus::Cancelled), (A, QueueItemStatus::Extracting)]
      );
    });
  }

  #[gpui_kit::test]
  fn cancel_requests_flag_only_active_items(cx: &mut TestAppContext) {
    let harness = Harness::new(cx);
    harness.queue.update(cx, |queue, cx| {
      queue.add_to_queue(A, cx);
      queue.mark_cancel_requested(A, cx);
      queue.mark_cancel_requested(B, cx);
      assert!(queue.item(A).unwrap().cancel_requested);
      assert!(queue.item(B).is_none());
    });
    harness.apply(cx, finished(A, ExtractOutcome::Cancelled, 1));
    harness.take_events();
    harness.queue.update(cx, |queue, cx| queue.mark_cancel_requested(A, cx));
    assert_eq!(harness.take_events(), [], "a finished item is left alone");
  }

  #[gpui_kit::test]
  fn removing_and_clearing(cx: &mut TestAppContext) {
    let harness = Harness::new(cx);
    harness.queue.update(cx, |queue, cx| {
      queue.add_to_queue(A, cx);
      queue.add_to_queue(B, cx);
      queue.remove_from_queue(A, cx);
      queue.remove_from_queue(A, cx);
      assert!(queue.item(A).is_none());
      assert_eq!(queue.items().count(), 1);
      queue.clear(cx);
      assert_eq!(queue.items().count(), 0);
      // The extract queue still has both.
      assert!(queue.has_active());
      assert_eq!(queue.active_files(), [A, B]);
      assert!(!queue.can_enqueue(A), "not before its extraction ended");
    });
    harness.apply(cx, started(B, 1));
    harness.apply(cx, finished(A, ExtractOutcome::Cancelled, 1));
    harness.apply(cx, finished(B, ExtractOutcome::Cancelled, 2));
    harness.queue.read_with(cx, |queue, _| {
      assert_eq!(queue.items().count(), 0, "their ends list nothing");
      assert!(!queue.has_active());
      assert!(queue.can_enqueue(A));
    });
  }

  /// A reservation ends when it is cancelled (the item finishes as
  /// cancelled), removed (the item goes) or handed over.
  #[gpui_kit::test]
  fn a_reservation_ends_by_cancel_removal_or_hand_over(cx: &mut TestAppContext) {
    let harness = Harness::new(cx);
    harness.queue.update(cx, |queue, cx| {
      let first = queue.reserve(A, cx).expect("reserved");
      assert_eq!(queue.reserve(A, cx), None, "waiting already");
      assert!(queue.is_reserved(A, first));
      assert!(queue.cancel_reservation(A, at(1), cx));
      assert_eq!(queue.item(A).unwrap().status, QueueItemStatus::Cancelled);
      assert!(!queue.is_reserved(A, first));
      assert!(!queue.cancel_reservation(A, at(1), cx), "nothing to cancel");
      let second = queue.reserve(A, cx).expect("reserved again");
      assert_ne!(second, first, "a reservation of its own");
      let b = queue.reserve(B, cx).expect("reserved");
      queue.remove_from_queue(B, cx);
      assert!(queue.item(B).is_none() && !queue.is_reserved(B, b), "gone at once");
      queue.hand_over(A, vec!["/out/a.srt".to_string()]);
      assert!(!queue.is_reserved(A, second));
      assert!(!queue.cancel_reservation(A, at(1), cx), "the extract queue has it");
      assert_eq!(queue.active_files(), [A]);
    });
    let cancelled = drained(&harness.take_events());
    assert_eq!(cancelled.len(), 1, "the cancelled reservation ended a batch");
    assert_eq!(cancelled[0].cancelled, 1);
  }

  /// A handed-over extraction owns its outputs until it finishes, also when
  /// its file was removed meanwhile.
  #[gpui_kit::test]
  fn outputs_are_taken_until_the_extraction_finishes(cx: &mut TestAppContext) {
    let harness = Harness::new(cx);
    let (a_srt, b_srt) = ("/out/a.srt".to_string(), "/out/b.srt".to_string());
    harness.queue.update(cx, |queue, cx| {
      queue.reserve(A, cx);
      queue.reserve(B, cx);
      queue.hand_over(A, vec![a_srt.clone()]);
      queue.hand_over(B, vec![b_srt.clone()]);
      assert_eq!(
        queue.output_owner(B, &["/out/x.srt".to_string(), a_srt.clone()]),
        Some((1, A))
      );
      assert_eq!(queue.output_owner(A, std::slice::from_ref(&a_srt)), None, "its own");
      queue.remove_from_queue(B, cx);
      assert_eq!(
        queue.output_owner(A, std::slice::from_ref(&b_srt)),
        Some((0, B)),
        "still extracting"
      );
    });
    harness.apply(cx, finished(A, ExtractOutcome::Completed, 2));
    harness.apply(cx, finished(B, ExtractOutcome::Cancelled, 2));
    harness.queue.read_with(cx, |queue, _| {
      assert_eq!(queue.output_owner(B, &[a_srt.clone(), b_srt.clone()]), None);
    });
  }

  #[gpui_kit::test]
  fn a_refused_file_leaves_the_queue(cx: &mut TestAppContext) {
    let harness = Harness::new(cx);
    harness.queue.update(cx, |queue, cx| {
      queue.add_to_queue(A, cx);
      queue.add_to_queue(B, cx);
      // Refused by the extract queue.
      queue.abandon(B, cx);
      assert!(queue.item(B).is_none());
      assert!(queue.can_enqueue(B));
      assert_eq!(queue.active_files(), [A]);
    });
    harness.apply(cx, started(A, 0));
    harness.apply(cx, finished(A, ExtractOutcome::Completed, 3));
    harness.queue.read_with(cx, |queue, _| assert!(!queue.has_active()));
  }

  #[gpui_kit::test]
  fn clear_completed_removes_finished_items_across_drives(cx: &mut TestAppContext) {
    let harness = Harness::new(cx);
    let c = "/media/c.mkv";
    harness.queue.update(cx, |queue, cx| {
      for file in [A, B, c] {
        queue.add_to_queue(file, cx);
      }
    });
    harness.apply(cx, finished(A, ExtractOutcome::Completed, 1));
    harness.apply(cx, finished(c, ExtractOutcome::Failed, 2));
    harness.queue.update(cx, |queue, cx| {
      queue.move_to_drive(c, "another drive", cx);
      queue.clear_completed(cx);
      let left: Vec<&str> = queue.items().map(|item| item.file.as_str()).collect();
      assert_eq!(left, [B], "the waiting item stays");
      assert_eq!(queue.active_files(), [B]);
    });
  }

  #[gpui_kit::test]
  fn the_summary_counts_finished_items_and_averages_progress(cx: &mut TestAppContext) {
    let harness = Harness::new(cx);
    assert_eq!(
      harness.queue.read_with(cx, |queue, _| queue.summary()),
      QueueSummary::default()
    );
    let c = "/media/c.mkv";
    harness.queue.update(cx, |queue, cx| {
      for file in [A, B, c] {
        queue.add_to_queue(file, cx);
      }
    });
    harness.apply(cx, finished(A, ExtractOutcome::Cancelled, 1));
    harness.apply(cx, started(B, 1));
    harness.apply(cx, progress(B, 51));
    let summary = harness.queue.read_with(cx, |queue, _| queue.summary());
    assert_eq!(
      summary,
      QueueSummary {
        done: 1,
        total: 3,
        active: true,
        // (100 + 51 + 0) / 3, rounded down.
        percent: 50,
      }
    );
    harness.apply(cx, finished(B, ExtractOutcome::Completed, 2));
    harness.apply(cx, finished(c, ExtractOutcome::Completed, 2));
    let summary = harness.queue.read_with(cx, |queue, _| queue.summary());
    assert_eq!((summary.done, summary.active, summary.percent), (3, false, 100));
  }

  fn drained(events: &[QueueEvent]) -> Vec<QueueBatch> {
    events
      .iter()
      .filter_map(|event| match event {
        QueueEvent::Drained(batch) => Some(batch.clone()),
        _ => None,
      })
      .collect()
  }

  #[gpui_kit::test]
  fn a_batch_drains_once_after_its_last_extraction(cx: &mut TestAppContext) {
    let harness = Harness::new(cx);
    let c = "/media/c.mkv";
    harness.queue.update(cx, |queue, cx| {
      for file in [A, B, c] {
        queue.add_to_queue(file, cx);
      }
    });
    harness.apply(cx, started(A, 10));
    harness.apply(cx, finished(A, ExtractOutcome::Completed, 40));
    harness.apply(cx, started(B, 40));
    harness.apply(cx, finished(B, ExtractOutcome::Failed, 45));
    assert_eq!(drained(&harness.take_events()), [], "an extraction still waits");
    harness.apply(cx, started(c, 45));
    harness.apply(cx, finished(c, ExtractOutcome::Completed, 100));
    let batches = drained(&harness.take_events());
    assert_eq!(
      batches,
      [QueueBatch {
        completed: 2,
        failed: 1,
        cancelled: 0,
        started: Some(at(10)),
        ended: Some(at(100)),
        last: Some((c.to_string(), Duration::from_secs(55))),
      }]
    );
    assert_eq!(batches[0].elapsed(), Duration::from_secs(90));
    assert_eq!(batches[0].finished(), 3);
    assert!(batches[0].notifies());
    // A late event of a finished item starts no batch.
    harness.apply(cx, progress(c, 50));
    assert_eq!(drained(&harness.take_events()), []);
  }

  #[gpui_kit::test]
  fn the_next_batch_starts_afresh(cx: &mut TestAppContext) {
    let harness = Harness::new(cx);
    harness.queue.update(cx, |queue, cx| queue.add_to_queue(A, cx));
    harness.apply(cx, started(A, 0));
    harness.apply(cx, finished(A, ExtractOutcome::Completed, 5));
    harness.take_events();
    harness.queue.update(cx, |queue, cx| queue.add_to_queue(B, cx));
    harness.apply(cx, started(B, 20));
    harness.apply(cx, finished(B, ExtractOutcome::Completed, 23));
    let batches = drained(&harness.take_events());
    assert_eq!(batches.len(), 1);
    assert_eq!(
      (batches[0].completed, batches[0].elapsed()),
      (1, Duration::from_secs(3))
    );
    assert_eq!(batches[0].last, Some((B.to_string(), Duration::from_secs(3))));
  }

  #[gpui_kit::test]
  fn a_batch_cancelled_entirely_does_not_notify(cx: &mut TestAppContext) {
    let harness = Harness::new(cx);
    harness.queue.update(cx, |queue, cx| {
      queue.add_to_queue(A, cx);
      queue.add_to_queue(B, cx);
    });
    harness.apply(cx, started(A, 0));
    harness.apply(cx, finished(A, ExtractOutcome::Cancelled, 2));
    harness.apply(cx, finished(B, ExtractOutcome::Cancelled, 2));
    let batches = drained(&harness.take_events());
    assert_eq!(batches.len(), 1);
    assert_eq!(batches[0].cancelled, 2);
    assert!(!batches[0].notifies());
  }

  #[gpui_kit::test]
  fn removing_the_last_waiting_item_ends_the_batch_with_its_cancel(cx: &mut TestAppContext) {
    let harness = Harness::new(cx);
    harness.queue.update(cx, |queue, cx| {
      queue.add_to_queue(A, cx);
      queue.add_to_queue(B, cx);
    });
    harness.apply(cx, started(A, 0));
    harness.apply(cx, finished(A, ExtractOutcome::Completed, 8));
    harness.take_events();
    harness.queue.update(cx, |queue, cx| queue.remove_from_queue(B, cx));
    assert_eq!(drained(&harness.take_events()), [], "the extract queue still has B");
    // The late `Cancelled` of the removed item drops it: no row, no toast, no count.
    harness.apply(cx, finished(B, ExtractOutcome::Cancelled, 9));
    let events = harness.take_events();
    assert!(!events.iter().any(|event| matches!(event, QueueEvent::Finished { .. })));
    let batches = drained(&events);
    assert_eq!(batches.len(), 1);
    assert_eq!((batches[0].completed, batches[0].cancelled), (1, 0));
    assert_eq!(harness.status(cx, B), None);
  }

  #[gpui_kit::test]
  fn drives_keep_the_order_of_their_first_file(cx: &mut TestAppContext) {
    let harness = Harness::new(cx);
    harness.queue.update(cx, |queue, cx| {
      queue.add_to_queue(A, cx);
      queue.add_to_queue(B, cx);
      let drive = bme_core::extract::get_drive_key(A);
      assert_eq!(queue.drives(), [(drive.clone(), vec![A.to_string(), B.to_string()])]);
      let files: Vec<&str> = queue.items_in_drive(&drive).map(|item| item.file.as_str()).collect();
      assert_eq!(files, [A, B]);
      assert_eq!(queue.items_in_drive("elsewhere").count(), 0);
    });
  }

  #[gpui_kit::test]
  fn the_clock_ticks_only_while_an_extraction_runs(cx: &mut TestAppContext) {
    let harness = Harness::new(cx);
    harness.queue.update(cx, |queue, cx| queue.add_to_queue(A, cx));
    cx.executor().advance_clock(Duration::from_secs(3));
    cx.run_until_parked();
    harness.take_events();
    harness.apply(cx, started(A, 0));
    harness.take_events();
    cx.executor().advance_clock(Duration::from_millis(2_500));
    cx.run_until_parked();
    assert_eq!(harness.take_events(), [QueueEvent::Tick, QueueEvent::Tick]);
    harness.apply(cx, finished(A, ExtractOutcome::Completed, 3));
    harness.take_events();
    cx.executor().advance_clock(Duration::from_secs(3));
    cx.run_until_parked();
    assert_eq!(harness.take_events(), []);
  }
}
