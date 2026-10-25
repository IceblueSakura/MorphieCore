//! One private, locked, bounded JSONL writer; request paths only try-send.
#[cfg(test)]
use super::Record;
use serde::Serialize;
use std::{
    fs::{File, OpenOptions},
    io::{self, Read, Seek, SeekFrom, Write},
    path::{Path, PathBuf},
    sync::{
        Arc,
        atomic::{AtomicBool, AtomicU64, Ordering},
        mpsc,
    },
    time::{Duration, Instant},
};

const LIMIT: usize = 1 << 20;
const PREFIX: &[u8] = b"{\"kind\":\"morphiecore_diagnostics\",\"version\":1,";
const HEADER: &[u8] = b"{\"kind\":\"morphiecore_diagnostics\",\"version\":1,\"event\":\"file\"}\n";
const RECORD_LIMIT: usize = LIMIT - HEADER.len();

#[derive(Default)]
struct Counters {
    dropped: AtomicU64,
    failed: AtomicU64,
    flush_incomplete: AtomicU64,
    recovered_tail_bytes: AtomicU64,
    writer_failed: AtomicBool,
}
#[derive(Clone, Copy, Default, Eq, PartialEq, Serialize)]
struct Health {
    dropped: u64,
    failed: u64,
    flush_incomplete: u64,
    recovered_tail_bytes: u64,
    writer_failed: bool,
}
impl Counters {
    fn snapshot(&self) -> Health {
        Health {
            dropped: self.dropped.load(Ordering::Relaxed),
            failed: self.failed.load(Ordering::Relaxed),
            flush_incomplete: self.flush_incomplete.load(Ordering::Relaxed),
            recovered_tail_bytes: self.recovered_tail_bytes.load(Ordering::Relaxed),
            writer_failed: self.writer_failed.load(Ordering::Relaxed),
        }
    }
    fn fail(&self) {
        self.failed.fetch_add(1, Ordering::Relaxed);
        self.writer_failed.store(true, Ordering::Relaxed);
    }
}
struct Shared {
    tx: mpsc::SyncSender<Message>,
    run: String,
    next: AtomicU64,
    counts: Arc<Counters>,
}
#[derive(Clone)]
pub(in crate::gateway) struct Sink(Arc<Shared>, Arc<super::telemetry::Telemetry>);
#[derive(Clone)]
pub(super) struct Output(Arc<Shared>);
pub(super) enum Message {
    #[cfg(test)]
    Record(Box<Record>),
    Log(Box<super::export::LocalLog>),
    Span(Box<super::export::LocalSpan>),
    Flush(tokio::sync::oneshot::Sender<bool>),
}
#[derive(Serialize)]
struct Line<'a, T> {
    kind: &'static str,
    version: u8,
    event: &'static str,
    #[serde(flatten)]
    value: &'a T,
}
fn encode(event: &'static str, value: &impl Serialize) -> io::Result<Vec<u8>> {
    let mut bytes = serde_json::to_vec(&Line {
        kind: "morphiecore_diagnostics",
        version: 1,
        event,
        value,
    })
    .map_err(io::Error::other)?;
    bytes.push(b'\n');
    Ok(bytes)
}
fn warn(health: Health) {
    // Only fixed labels and counts, never a path, I/O error or record contents.
    let _ = writeln!(
        io::stderr().lock(),
        "MorphieCore diagnostics: dropped={} failed={} flush_incomplete={} recovered_tail_bytes={} writer_failed={}",
        health.dropped,
        health.failed,
        health.flush_incomplete,
        health.recovered_tail_bytes,
        health.writer_failed
    );
}
impl Sink {
    pub(in crate::gateway) fn open(path: &Path) -> io::Result<Self> {
        let counts = Arc::new(Counters::default());
        let (tx, rx) = mpsc::sync_channel(64);
        let sink = Self::from_parts(tx, counts.clone())?;
        let writer = Rolling::open(path, counts)?;
        let run = sink.0.run.clone();
        let counts = sink.0.counts.clone();
        std::thread::Builder::new()
            .name("gateway-metadata".into())
            .spawn(move || work(writer, rx, counts, run))?;
        Ok(sink)
    }
    fn from_parts(tx: mpsc::SyncSender<Message>, counts: Arc<Counters>) -> io::Result<Self> {
        let mut nonce = [0u8; 16];
        getrandom::fill(&mut nonce).map_err(|_| io::Error::other("diagnostics randomness"))?;
        let run = nonce.iter().map(|b| format!("{b:02x}")).collect();
        let shared = Arc::new(Shared {
            tx,
            run,
            next: AtomicU64::new(1),
            counts,
        });
        let telemetry = super::telemetry::Telemetry::new(Output(shared.clone()));
        Ok(Self(shared, telemetry))
    }
    #[cfg(test)]
    pub(super) fn from_sender(tx: mpsc::SyncSender<Message>) -> Self {
        Self::from_parts(tx, Arc::default()).unwrap()
    }
    pub(super) fn request_id(&self) -> Option<String> {
        self.0
            .next
            .try_update(Ordering::Relaxed, Ordering::Relaxed, |n| n.checked_add(1))
            .ok()
            .map(|n| format!("{}:{n}", self.0.run))
    }
    #[cfg(test)]
    pub(super) fn record(&self, record: Box<Record>) {
        Output(self.0.clone()).send(Message::Record(record));
    }
    pub(super) fn request(&self, id: &str) -> super::telemetry::Request {
        self.1.request(id)
    }
    pub(in crate::gateway) async fn flush(&self) {
        Output(self.0.clone()).flush().await;
    }
}
impl Output {
    pub(super) async fn flush(&self) {
        let (tx, rx) = tokio::sync::oneshot::channel();
        let complete = self.0.tx.try_send(Message::Flush(tx)).is_ok()
            && matches!(
                tokio::time::timeout(Duration::from_secs(2), rx).await,
                Ok(Ok(true))
            );
        if !complete {
            self.0
                .counts
                .flush_incomplete
                .fetch_add(1, Ordering::Relaxed);
        }
    }
    pub(super) fn encoding_failed(&self) {
        self.0.counts.failed.fetch_add(1, Ordering::Relaxed);
    }
    pub(super) fn send(&self, message: Message) -> bool {
        if self.0.counts.writer_failed.load(Ordering::Relaxed) {
            self.0.counts.failed.fetch_add(1, Ordering::Relaxed);
            return false;
        }
        match self.0.tx.try_send(message) {
            Ok(()) => true,
            Err(mpsc::TrySendError::Full(_)) => {
                self.0.counts.dropped.fetch_add(1, Ordering::Relaxed);
                false
            }
            Err(mpsc::TrySendError::Disconnected(_)) => {
                self.0.counts.fail();
                false
            }
        }
    }
}

fn work(mut writer: impl Write, rx: mpsc::Receiver<Message>, counts: Arc<Counters>, run: String) {
    #[derive(Serialize)]
    struct Status<'a> {
        run_id: &'a str,
        #[serde(flatten)]
        health: Health,
    }
    let mut reported = Health::default();
    let mut warned = None::<Instant>;
    loop {
        let message = rx.recv_timeout(Duration::from_millis(100));
        let force = matches!(
            message,
            Ok(Message::Flush(_)) | Err(mpsc::RecvTimeoutError::Disconnected)
        );
        let encoded = match &message {
            #[cfg(test)]
            Ok(Message::Record(record)) => Some(encode("request", record.as_ref())),
            Ok(Message::Log(log)) => Some(
                log.json()
                    .map_err(|_| io::Error::other("diagnostics encoding"))
                    .and_then(|(event, value)| encode(event, &value)),
            ),
            Ok(Message::Span(span)) => Some(encode("span", &span.json())),
            _ => None,
        };
        if let Some(encoded) = encoded {
            if counts.writer_failed.load(Ordering::Relaxed) {
                counts.failed.fetch_add(1, Ordering::Relaxed);
            } else {
                match encoded {
                    Ok(bytes) if bytes.len() > RECORD_LIMIT => {
                        counts.dropped.fetch_add(1, Ordering::Relaxed);
                    }
                    Ok(bytes) => {
                        if writer.write_all(&bytes).is_err() {
                            counts.fail();
                        }
                    }
                    Err(_) => {
                        counts.failed.fetch_add(1, Ordering::Relaxed);
                    }
                }
            }
        }
        let health = counts.snapshot();
        if health != reported
            && (force
                || !reported.writer_failed && health.writer_failed
                || warned.is_none_or(|last| last.elapsed() >= Duration::from_secs(1)))
        {
            reported = health;
            if !health.writer_failed {
                let result = encode(
                    "logging_status",
                    &Status {
                        run_id: &run,
                        health,
                    },
                )
                .and_then(|bytes| writer.write_all(&bytes));
                if result.is_err() {
                    counts.fail();
                    reported = counts.snapshot();
                }
            }
            // A healthy file records losses without depending on stderr. Once
            // file output has failed, only the background worker warns there.
            if reported.writer_failed {
                warn(reported);
            }
            warned = Some(Instant::now());
        }
        match message {
            Ok(Message::Flush(done)) => {
                let complete = if counts.writer_failed.load(Ordering::Relaxed) {
                    false
                } else if writer.flush().is_err() {
                    counts.fail();
                    false
                } else {
                    true
                };
                let _ = done.send(complete);
            }
            Err(mpsc::RecvTimeoutError::Disconnected) => {
                if !counts.writer_failed.load(Ordering::Relaxed) && writer.flush().is_err() {
                    counts.fail();
                    warn(counts.snapshot());
                }
                break;
            }
            _ => {}
        }
    }
}

struct Rolling {
    path: PathBuf,
    archive: PathBuf,
    file: Option<File>,
    used: usize,
    _locks: [File; 2],
    mistrust: fs_mistrust::Mistrust,
}
fn sibling(path: &Path, suffix: &str) -> io::Result<PathBuf> {
    let mut name = path
        .file_name()
        .ok_or_else(|| io::Error::other("diagnostics path"))?
        .to_os_string();
    name.push(suffix);
    Ok(path.with_file_name(name))
}
fn options() -> OpenOptions {
    let mut options = OpenOptions::new();
    options.read(true).append(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    options
}
fn check(path: &Path, mistrust: &fs_mistrust::Mistrust) -> io::Result<Option<u64>> {
    let metadata = match path.symlink_metadata() {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(error),
    };
    if !metadata.is_file() || metadata.is_symlink() {
        return Err(io::Error::other("diagnostics file"));
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        if metadata.nlink() != 1 || metadata.mode() & 0o077 != 0 {
            return Err(io::Error::other("diagnostics permissions"));
        }
    }
    mistrust
        .verifier()
        .require_file()
        .check(path)
        .map_err(|_| io::Error::other("diagnostics permissions"))?;
    Ok(Some(metadata.len()))
}
fn existing(path: &Path, mistrust: &fs_mistrust::Mistrust) -> io::Result<Option<File>> {
    let Some(size) = check(path, mistrust)? else {
        return Ok(None);
    };
    if size == 0 || size > LIMIT as u64 {
        return Err(io::Error::other("diagnostics size"));
    }
    let mut file = options().open(path)?;
    let mut prefix = [0u8; PREFIX.len()];
    file.read_exact(&mut prefix)?;
    if prefix != PREFIX {
        return Err(io::Error::other("diagnostics format"));
    }
    Ok(Some(file))
}
fn create(path: &Path) -> io::Result<File> {
    let mut file = options().create_new(true).open(path)?;
    file.write_all(HEADER)?;
    Ok(file)
}
fn lock(path: &Path, mistrust: &fs_mistrust::Mistrust) -> io::Result<File> {
    let path = sibling(path, ".lock")?;
    let file = match check(&path, mistrust)? {
        Some(0) => options().open(&path)?,
        Some(_) => return Err(io::Error::other("diagnostics lock format")),
        None => options().create_new(true).open(&path)?,
    };
    file.try_lock()
        .map_err(|_| io::Error::other("diagnostics writer unavailable"))?;
    Ok(file)
}
impl Rolling {
    fn open(path: &Path, counts: Arc<Counters>) -> io::Result<Self> {
        let parent = path
            .parent()
            .filter(|p| !p.as_os_str().is_empty())
            .unwrap_or(Path::new("."));
        let mut builder = fs_mistrust::Mistrust::builder();
        builder.ignore_environment();
        #[cfg(unix)]
        builder.trust_no_group_id();
        let mistrust = builder
            .build()
            .map_err(|_| io::Error::other("diagnostics directory"))?;
        mistrust
            .verifier()
            .permit_readable()
            .require_directory()
            .check(parent)
            .map_err(|_| io::Error::other("diagnostics directory must be private"))?;
        let archive = sibling(path, ".1")?;
        // Lock both data slots: another base path may otherwise name our archive.
        let locks = [lock(path, &mistrust)?, lock(&archive, &mistrust)?];
        existing(&archive, &mistrust)?;
        let mut file = match existing(path, &mistrust)? {
            Some(file) => file,
            None => create(path)?,
        };
        // Only an identified diagnostics file is repaired; never read arbitrary files.
        let mut bytes = Vec::new();
        file.seek(SeekFrom::Start(0))?;
        Read::take(&mut file, LIMIT as u64 + 1).read_to_end(&mut bytes)?;
        if bytes.len() > LIMIT {
            return Err(io::Error::other("diagnostics size"));
        }
        let mut used = bytes.iter().rposition(|b| *b == b'\n').map_or(0, |p| p + 1);
        if used != bytes.len() {
            file.set_len(used as u64)?;
            counts
                .recovered_tail_bytes
                .store((bytes.len() - used) as u64, Ordering::Relaxed);
            if used == 0 {
                file.write_all(HEADER)?;
                used = HEADER.len();
            }
        }
        Ok(Self {
            path: path.into(),
            archive,
            file: Some(file),
            used,
            _locks: locks,
            mistrust,
        })
    }
}
impl Write for Rolling {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        if bytes.len() > RECORD_LIMIT {
            return Err(io::Error::other("diagnostics record size"));
        }
        if bytes.len() > LIMIT.saturating_sub(self.used) {
            let current = existing(&self.path, &self.mistrust)?
                .ok_or_else(|| io::Error::other("diagnostics file unavailable"))?;
            if current.metadata()?.len() != self.used as u64 {
                return Err(io::Error::other("diagnostics file changed"));
            }
            drop(current);
            if existing(&self.archive, &self.mistrust)?.is_some() {
                std::fs::remove_file(&self.archive)?;
            }
            drop(self.file.take());
            std::fs::rename(&self.path, &self.archive)?;
            self.file = Some(create(&self.path)?);
            self.used = HEADER.len();
        }
        self.file
            .as_mut()
            .ok_or_else(|| io::Error::other("diagnostics file unavailable"))?
            .write_all(bytes)?;
        self.used += bytes.len();
        Ok(bytes.len())
    }
    fn flush(&mut self) -> io::Result<()> {
        self.file
            .as_mut()
            .ok_or_else(|| io::Error::other("diagnostics file unavailable"))?
            .flush()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::{Value, json};

    #[test]
    fn rotation_is_record_aligned_bounded_and_restartable_with_one_writer() {
        let directory = crate::test_files::private_directory();
        let path = directory.path().join("events.jsonl");
        let counts = Arc::new(Counters::default());
        let mut writer = Rolling::open(&path, counts.clone()).unwrap();
        assert!(Rolling::open(&path, counts.clone()).is_err());
        for sequence in 0..7 {
            writer
                .write_all(
                    &encode(
                        "request",
                        &json!({
                            "sequence":sequence, "padding":"x".repeat(LIMIT / 3)
                        }),
                    )
                    .unwrap(),
                )
                .unwrap();
        }
        let archive = sibling(&path, ".1").unwrap();
        for file in [&path, &archive] {
            assert!(file.metadata().unwrap().len() <= LIMIT as u64);
            for line in std::fs::read_to_string(file).unwrap().lines() {
                let value: Value = serde_json::from_str(line).unwrap();
                assert_eq!(value["kind"], "morphiecore_diagnostics");
                if value["event"] == "request" {
                    assert!(value["sequence"].as_u64().unwrap() >= 4);
                } else {
                    assert_eq!(value["event"], "file");
                }
            }
        }
        drop(writer);
        let mut resumed = Rolling::open(&path, counts).unwrap();
        resumed
            .write_all(&encode("request", &json!({"sequence":7})).unwrap())
            .unwrap();
        let lines = std::fs::read_to_string(&path).unwrap();
        assert_eq!(lines.lines().count(), 3);
        assert_eq!(
            serde_json::from_str::<Value>(lines.lines().last().unwrap()).unwrap()["sequence"],
            7
        );
        #[cfg(unix)]
        {
            use std::os::unix::fs::MetadataExt;
            assert_eq!(path.metadata().unwrap().mode() & 0o777, 0o600);
        }
    }

    #[test]
    fn identified_partial_tail_is_counted_and_repaired_without_losing_complete_records() {
        let directory = crate::test_files::private_directory();
        let path = directory.path().join("events.jsonl");
        let complete = encode("request", &json!({"sequence":1})).unwrap();
        let tail = [PREFIX, b"\"event\":\"request\",\"elapsed_ms\":"].concat();
        let mut file = options().create_new(true).open(&path).unwrap();
        file.write_all(&complete).unwrap();
        file.write_all(&tail).unwrap();
        drop(file);
        let counts = Arc::new(Counters::default());
        let mut writer = Rolling::open(&path, counts.clone()).unwrap();
        assert_eq!(counts.snapshot().recovered_tail_bytes, tail.len() as u64);
        writer
            .write_all(&encode("request", &json!({"sequence":2})).unwrap())
            .unwrap();
        let data = std::fs::read_to_string(path).unwrap();
        let lines: Vec<Value> = data
            .lines()
            .map(|s| serde_json::from_str(s).unwrap())
            .collect();
        assert_eq!(lines.len(), 2);
        assert_eq!(lines[0]["sequence"], 1);
        assert_eq!(lines[1]["sequence"], 2);
    }

    #[test]
    fn unrelated_files_and_archives_are_not_replaced() {
        for suffix in ["", ".1"] {
            for bytes in [b"".as_slice(), b"synthetic unrelated metadata".as_slice()] {
                let directory = crate::test_files::private_directory();
                let path = directory.path().join("events.jsonl");
                let protected = sibling(&path, suffix).unwrap();
                options()
                    .create_new(true)
                    .open(&protected)
                    .unwrap()
                    .write_all(bytes)
                    .unwrap();
                assert!(Rolling::open(&path, Arc::default()).is_err());
                assert_eq!(std::fs::read(&protected).unwrap(), bytes);
            }
        }
    }

    #[test]
    fn overlapping_data_namespaces_reject_before_mutating_another_writer() {
        for archive_first in [false, true] {
            let directory = crate::test_files::private_directory();
            let base = directory.path().join("events.jsonl");
            let archive = sibling(&base, ".1").unwrap();
            let (first, second) = if archive_first {
                (&archive, &base)
            } else {
                (&base, &archive)
            };
            let mut writer = Rolling::open(first, Arc::default()).unwrap();
            let original = encode("request", &json!({"sequence":1})).unwrap();
            writer.write_all(&original).unwrap();
            let before = std::fs::read(first).unwrap();
            assert!(Rolling::open(second, Arc::default()).is_err());
            assert_eq!(std::fs::read(first).unwrap(), before);
            assert!(!second.exists());
            writer
                .write_all(&encode("request", &json!({"sequence":2})).unwrap())
                .unwrap();
            assert!(
                std::fs::read_to_string(first)
                    .unwrap()
                    .contains("\"sequence\":2")
            );
        }
    }

    #[cfg(unix)]
    #[test]
    fn file_aliases_are_rejected_without_following_or_modifying_them() {
        use std::os::unix::fs::symlink;
        for hard in [false, true] {
            let directory = crate::test_files::private_directory();
            let path = directory.path().join("events.jsonl");
            let target = directory.path().join("target");
            let original = encode("request", &json!({"sequence":1})).unwrap();
            options()
                .create_new(true)
                .open(&target)
                .unwrap()
                .write_all(&original)
                .unwrap();
            if hard {
                std::fs::hard_link(&target, &path).unwrap();
            } else {
                symlink(&target, &path).unwrap();
            }
            assert!(Rolling::open(&path, Arc::default()).is_err());
            assert_eq!(std::fs::read(target).unwrap(), original);
        }
    }

    #[tokio::test]
    async fn queue_loss_disconnect_and_incomplete_flush_are_observable_without_waiting() {
        let (tx, rx) = mpsc::sync_channel(1);
        let sink = Sink::from_sender(tx);
        let first = sink.request_id().unwrap();
        let second = sink.request_id().unwrap();
        assert_ne!(first, second);
        assert_eq!(first.split(':').next(), second.split(':').next());
        sink.record(Box::default());
        sink.record(Box::default());
        assert_eq!(sink.0.counts.snapshot().dropped, 1);
        sink.flush().await;
        assert_eq!(sink.0.counts.snapshot().flush_incomplete, 1);
        drop(rx);
        sink.record(Box::default());
        assert!(sink.0.counts.snapshot().writer_failed);
        assert_eq!(sink.0.counts.snapshot().failed, 1);
        let (tx, _rx) = mpsc::sync_channel(1);
        let restarted = Sink::from_sender(tx);
        assert_ne!(
            first.split(':').next(),
            restarted.request_id().unwrap().split(':').next()
        );
    }

    #[test]
    fn health_updates_during_a_status_write_are_not_marked_as_already_emitted() {
        struct Gated {
            bytes: Arc<std::sync::Mutex<Vec<u8>>>,
            gate: Option<(mpsc::SyncSender<()>, mpsc::Receiver<()>)>,
        }
        impl Write for Gated {
            fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
                if std::str::from_utf8(bytes)
                    .unwrap()
                    .contains("\"event\":\"logging_status\"")
                    && let Some((ready, release)) = self.gate.take()
                {
                    ready.send(()).map_err(io::Error::other)?;
                    release
                        .recv_timeout(Duration::from_secs(2))
                        .map_err(io::Error::other)?;
                }
                self.bytes.lock().unwrap().extend_from_slice(bytes);
                Ok(bytes.len())
            }
            fn flush(&mut self) -> io::Result<()> {
                Ok(())
            }
        }
        let (tx, rx) = mpsc::sync_channel(2);
        let counts = Arc::new(Counters::default());
        counts.dropped.store(1, Ordering::Relaxed);
        let sink = Sink::from_parts(tx, counts.clone()).unwrap();
        let (ready, started) = mpsc::sync_channel(1);
        let (release, wait) = mpsc::channel();
        let bytes = Arc::new(std::sync::Mutex::new(vec![]));
        let writer = Gated {
            bytes: bytes.clone(),
            gate: Some((ready, wait)),
        };
        let observed = counts.clone();
        let worker = std::thread::spawn(move || work(writer, rx, observed, "test-run".into()));
        let (done, finished) = tokio::sync::oneshot::channel();
        sink.0.tx.send(Message::Flush(done)).unwrap();
        started.recv_timeout(Duration::from_secs(2)).unwrap();
        counts.dropped.fetch_add(1, Ordering::Relaxed);
        release.send(()).unwrap();
        assert!(finished.blocking_recv().unwrap());
        let (done, finished) = tokio::sync::oneshot::channel();
        sink.0.tx.send(Message::Flush(done)).unwrap();
        assert!(finished.blocking_recv().unwrap());
        drop(sink);
        worker.join().unwrap();
        let text = String::from_utf8(bytes.lock().unwrap().clone()).unwrap();
        let totals: Vec<u64> = text
            .lines()
            .map(|s| {
                serde_json::from_str::<Value>(s).unwrap()["dropped"]
                    .as_u64()
                    .unwrap()
            })
            .collect();
        assert_eq!(totals, vec![1, 2]);
    }

    struct FailedWriter;
    impl Write for FailedWriter {
        fn write(&mut self, _: &[u8]) -> io::Result<usize> {
            Err(io::Error::other("synthetic-private-io-error"))
        }
        fn flush(&mut self) -> io::Result<()> {
            Err(io::Error::other("synthetic-private-io-error"))
        }
    }
    #[tokio::test]
    async fn worker_write_failure_remains_failed_and_never_acknowledges_flush() {
        let (tx, rx) = mpsc::sync_channel(2);
        let counts = Arc::new(Counters::default());
        let sink = Sink::from_parts(tx, counts.clone()).unwrap();
        let observed = counts.clone();
        let worker =
            std::thread::spawn(move || work(FailedWriter, rx, observed, "synthetic-run".into()));
        sink.record(Box::default());
        sink.flush().await;
        assert!(counts.snapshot().writer_failed);
        assert_eq!(counts.snapshot().failed, 1);
        assert_eq!(counts.snapshot().flush_incomplete, 1);
        sink.record(Box::default());
        assert_eq!(counts.snapshot().failed, 2);
        assert!(
            !serde_json::to_string(&counts.snapshot())
                .unwrap()
                .contains("synthetic-private")
        );
        drop(sink);
        worker.join().unwrap();
    }
}
