use super::core::ReplayCacheDatabase;
use s2coop_analyzer::cache_overall_stats_generator::CacheReplayEntry;
use s2coop_analyzer::detailed_replay_analysis::{CacheEntrySink, CacheEntrySinkError};
use std::fmt::{Display, Formatter};
use std::path::PathBuf;
use std::sync::mpsc::{self, Receiver, Sender};
use std::thread::{self, JoinHandle};

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct ReplayCacheWriteResult {
    persisted_entries: usize,
    failed_batches: usize,
}

impl ReplayCacheWriteResult {
    fn add_persisted_entries(&mut self, value: usize) {
        self.persisted_entries = self.persisted_entries.saturating_add(value);
    }

    fn increment_failed_batches(&mut self) {
        self.failed_batches = self.failed_batches.saturating_add(1);
    }

    pub fn persisted_entries(&self) -> usize {
        self.persisted_entries
    }

    pub fn failed_batches(&self) -> usize {
        self.failed_batches
    }
}

#[derive(Clone, Debug)]
pub struct ReplayCacheWriteSender {
    sender: Sender<ReplayCacheWriteCommand>,
}

impl ReplayCacheWriteSender {
    fn new(sender: Sender<ReplayCacheWriteCommand>) -> Self {
        Self { sender }
    }

    pub fn write_entries(
        &self,
        entries: Vec<CacheReplayEntry>,
    ) -> Result<(), ReplayCacheWriteSendError> {
        if entries.is_empty() {
            return Ok(());
        }
        self.sender
            .send(ReplayCacheWriteCommand::new_async(entries))
            .map_err(|_| ReplayCacheWriteSendError::Closed)
    }

    pub fn write_entries_and_wait(
        &self,
        entries: Vec<CacheReplayEntry>,
    ) -> Result<usize, ReplayCacheWriteSendError> {
        if entries.is_empty() {
            return Ok(0);
        }

        let (result_sender, result_receiver) = mpsc::channel::<Result<usize, String>>();
        self.sender
            .send(ReplayCacheWriteCommand::new_blocking(
                entries,
                result_sender,
            ))
            .map_err(|_| ReplayCacheWriteSendError::Closed)?;
        result_receiver
            .recv()
            .map_err(|_| ReplayCacheWriteSendError::ResponseClosed)?
            .map_err(ReplayCacheWriteSendError::WriteFailed)
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ReplayCacheWriteSendError {
    Closed,
    ResponseClosed,
    WriteFailed(String),
}

impl Display for ReplayCacheWriteSendError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Closed => write!(formatter, "cache writer queue is closed"),
            Self::ResponseClosed => write!(formatter, "cache writer response channel is closed"),
            Self::WriteFailed(message) => formatter.write_str(message),
        }
    }
}

#[derive(Debug)]
struct ReplayCacheWriteCommand {
    entries: Vec<CacheReplayEntry>,
    result_sender: Option<Sender<Result<usize, String>>>,
}

impl ReplayCacheWriteCommand {
    fn new_async(entries: Vec<CacheReplayEntry>) -> Self {
        Self {
            entries,
            result_sender: None,
        }
    }

    fn new_blocking(
        entries: Vec<CacheReplayEntry>,
        result_sender: Sender<Result<usize, String>>,
    ) -> Self {
        Self {
            entries,
            result_sender: Some(result_sender),
        }
    }

    fn entries(&self) -> &[CacheReplayEntry] {
        &self.entries
    }

    fn result_sender(self) -> Option<Sender<Result<usize, String>>> {
        self.result_sender
    }
}

#[derive(Clone, Debug)]
pub struct QueuedReplayCacheEntrySink {
    sender: ReplayCacheWriteSender,
}

impl QueuedReplayCacheEntrySink {
    pub fn new(sender: ReplayCacheWriteSender) -> Self {
        Self { sender }
    }
}

impl CacheEntrySink for QueuedReplayCacheEntrySink {
    fn write_entries(&self, entries: &[CacheReplayEntry]) -> Result<usize, CacheEntrySinkError> {
        if entries.is_empty() {
            return Ok(0);
        }
        self.sender
            .write_entries(entries.to_vec())
            .map_err(|error| CacheEntrySinkError::new(error.to_string()))?;
        Ok(entries.len())
    }
}

pub struct ReplayCacheWriteQueue {
    sender: Option<ReplayCacheWriteSender>,
    handle: Option<JoinHandle<ReplayCacheWriteResult>>,
}

impl ReplayCacheWriteQueue {
    pub fn start(cache_path: impl Into<PathBuf>) -> Self {
        let cache_path = cache_path.into();
        let (sender, receiver) = mpsc::channel::<ReplayCacheWriteCommand>();
        let handle = thread::spawn(move || Self::run(cache_path, receiver));
        Self {
            sender: Some(ReplayCacheWriteSender::new(sender)),
            handle: Some(handle),
        }
    }

    pub fn write_entries_to_path(
        cache_path: impl Into<PathBuf>,
        entries: &[CacheReplayEntry],
    ) -> Result<ReplayCacheWriteResult, ReplayCacheWriteSendError> {
        let queue = Self::start(cache_path);
        let sender = queue.sender();
        let write_result = sender.write_entries_and_wait(entries.to_vec());
        drop(sender);
        let queue_result = queue.finish();
        write_result.map(|_| queue_result)
    }

    pub fn sender(&self) -> ReplayCacheWriteSender {
        self.sender
            .as_ref()
            .expect("cache writer sender should exist until finish")
            .clone()
    }

    pub fn finish(mut self) -> ReplayCacheWriteResult {
        self.sender.take();
        match self
            .handle
            .take()
            .expect("cache writer thread should exist until finish")
            .join()
        {
            Ok(result) => result,
            Err(_) => {
                crate::sco_log!("[SCO/cache] cache writer thread panicked");
                ReplayCacheWriteResult::default()
            }
        }
    }

    fn run(
        cache_path: PathBuf,
        receiver: Receiver<ReplayCacheWriteCommand>,
    ) -> ReplayCacheWriteResult {
        let mut result = ReplayCacheWriteResult::default();
        let mut database = match ReplayCacheDatabase::open_for_cache_path(&cache_path) {
            Ok(database) => database,
            Err(error) => {
                crate::sco_log!("[SCO/cache] failed to open cache writer: {error}");
                for command in receiver {
                    if !command.entries().is_empty() {
                        result.increment_failed_batches();
                    }
                    if let Some(result_sender) = command.result_sender() {
                        let _ = result_sender.send(Err(error.to_string()));
                    }
                }
                return result;
            }
        };

        for command in receiver {
            match database.upsert_entries_preserving_detailed(command.entries()) {
                Ok(changed) => {
                    result.add_persisted_entries(changed);
                    if let Some(result_sender) = command.result_sender() {
                        let _ = result_sender.send(Ok(changed));
                    }
                }
                Err(error) => {
                    result.increment_failed_batches();
                    crate::sco_log!("[SCO/cache] failed to persist cache writer batch: {error}");
                    if let Some(result_sender) = command.result_sender() {
                        let _ = result_sender.send(Err(error.to_string()));
                    }
                }
            }
        }
        result
    }
}
