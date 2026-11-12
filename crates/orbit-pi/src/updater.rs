//! Background, signed, in-app updates.
//!
//! Orbit ships the same EdDSA-signed appcast contract on every platform, with
//! no Sparkle dependency: a per-architecture feed names a release, the
//! artifact beside it carries a signature, and this module verifies the
//! signature against the public key `build.rs` compiled in before anything is
//! staged. Checks run on their own threads and report back through a channel,
//! so the UI only ever sees [`UpdateStatus`] and [`UpdaterEvent`].
//!
//! A check only reads the feed: it names the release, its notes, and the
//! artifact URL, and no bytes move until
//! [`Updater::download_available_update`] is asked for them. That keeps a
//! silent launch check to one small request, and means the version and
//! changelog the modal shows always precede the download.
//!
//! Platform install seams:
//! - **macOS** — the feed points at a `.tar.gz` whose single top-level entry
//!   is `Orbit Pi.app`. The helper waits for the app to quit, swaps the
//!   bundle for the extracted one, relaunches it, and rolls back if the new
//!   build never signals a ready main window.
//! - **Linux** — the same swap against the managed install prefix.
//! - **Windows** — the feed points at an installer, which runs silently.
//!
//! Debug builds stay dormant so a dev `cargo run` never replaces itself with a
//! production install. `ORBIT_FORCE_UPDATER=1` arms the real flow; an empty or
//! invalid compiled-in key also leaves the updater off.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{mpsc, Arc, Mutex};

use base64::Engine as _;
use gpui::Global;

/// Environment variable the relaunched build reads to signal a live window.
/// The install helper that sets it is Unix-only (see [`run_install_helper`]).
#[cfg(unix)]
const RELAUNCH_READY_ENV: &str = "ORBIT_UPDATE_READY_FILE";
/// Hidden flag that turns the app binary into its own update helper.
#[cfg(unix)]
const INSTALL_HELPER_FLAG: &str = "--orbit-update-install";

/// App-wide handle to the updater, if this build can update itself.
pub struct UpdaterState(pub Option<Updater>);

impl Global for UpdaterState {}

/// The compact state the UI renders. Release details never enter a frame
/// path; they live on the worker thread that fetched them.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum UpdateStatus {
    #[default]
    Idle,
    /// The feed names a release newer than this build. Nothing has been
    /// downloaded; everything shown comes from the feed alone.
    Available,
    /// The offered artifact is being downloaded and staged.
    Downloading,
    /// A verified artifact is staged and waiting for the install handoff.
    Ready,
    /// The install helper owns the swap.
    Updating,
}

#[derive(Clone, Debug)]
pub enum UpdaterEvent {
    StatusChanged(UpdateStatus),
    /// Bytes moved so far and the total the feed declared. Throttled to about
    /// one event per percent, so a large artifact cannot flood the frame loop.
    Progress {
        downloaded: u64,
        total: u64,
    },
    UpToDate,
    Failed(String),
    /// The helper has validated its inputs and waits for the app's normal
    /// quit handlers to finish before it swaps directories. The UI quits.
    /// Only a build whose updater can hand off (Unix) ever emits this.
    #[cfg(unix)]
    QuitAndInstall,
}

/// The public half of the release signing key, read from
/// `ORBIT_UPDATE_PUBLIC_KEY` (or a bundled `Info.plist`) by `build.rs`.
const PUBLIC_ED_KEY: &str = env!("ORBIT_UPDATE_PUBLIC_KEY");

/// The release feed for this platform and architecture. `ORBIT_UPDATE_FEED_URL`
/// overrides it at build time. A Sparkle appcast cannot say which binary an
/// item is for, so the client picks its feed by target.
const FEED_URL: &str = match option_env!("ORBIT_UPDATE_FEED_URL") {
    Some(url) => url,
    None => DEFAULT_FEED_URL,
};

// Each release carries its signed appcasts as assets, and the app reads them
// through GitHub's stable `releases/latest/download/` alias for the newest
// published release. Keep these paths in sync with `.github/workflows/release.yml`.
#[cfg(all(target_os = "macos", target_arch = "aarch64"))]
const DEFAULT_FEED_URL: &str =
    "https://github.com/imrj05/orbit/releases/latest/download/appcast-macos-aarch64.xml";
#[cfg(all(target_os = "macos", not(target_arch = "aarch64")))]
const DEFAULT_FEED_URL: &str =
    "https://github.com/imrj05/orbit/releases/latest/download/appcast-macos-x86_64.xml";
#[cfg(all(target_os = "linux", target_arch = "aarch64"))]
const DEFAULT_FEED_URL: &str =
    "https://github.com/imrj05/orbit/releases/latest/download/appcast-linux-aarch64.xml";
#[cfg(all(target_os = "linux", not(target_arch = "aarch64")))]
const DEFAULT_FEED_URL: &str =
    "https://github.com/imrj05/orbit/releases/latest/download/appcast-linux-x86_64.xml";
#[cfg(all(target_os = "windows", target_arch = "aarch64"))]
const DEFAULT_FEED_URL: &str =
    "https://github.com/imrj05/orbit/releases/latest/download/appcast-windows-aarch64.xml";
#[cfg(all(target_os = "windows", not(target_arch = "aarch64")))]
const DEFAULT_FEED_URL: &str =
    "https://github.com/imrj05/orbit/releases/latest/download/appcast-windows-x86_64.xml";
#[cfg(not(any(target_os = "macos", target_os = "linux", target_os = "windows")))]
const DEFAULT_FEED_URL: &str = "";

/// A checked feed is a few kilobytes; an artifact may be a full bundle.
const MAX_FEED_BYTES: u64 = 1024 * 1024;
const MAX_ARTIFACT_BYTES: u64 = 512 * 1024 * 1024;
#[cfg(unix)]
const MAX_UNPACKED_BYTES: u64 = 1024 * 1024 * 1024;
#[cfg(unix)]
const MAX_ARCHIVE_ENTRIES: usize = 100_000;
#[cfg(unix)]
const MAX_ERROR_BYTES: u64 = 16 * 1024;

/// A verified artifact on disk. On Unix the payload is an extracted directory
/// that must outlive the helper's swap, so it cleans itself up unless handed
/// off.
struct StagedUpdate {
    /// Unix: the extraction root (the helper finds its single child). Windows:
    /// the installer executable.
    path: PathBuf,
    cleanup: Option<PathBuf>,
    /// The release version the payload carries, so the settings surfaces can
    /// name the download ("Download v0.0.3") without touching the feed again.
    version: Option<String>,
    /// The release's notes from the feed's `<description>`, shown in the
    /// update modal next to the version. `None` when the feed omitted them.
    notes: Option<String>,
}

impl StagedUpdate {
    fn new(path: PathBuf, cleanup: PathBuf) -> Self {
        Self {
            path,
            cleanup: Some(cleanup),
            version: None,
            notes: None,
        }
    }

    fn path(&self) -> &Path {
        &self.path
    }

    /// Keep the staged payload alive for the helper, dropping the cleanup.
    fn disarm(mut self) {
        self.cleanup = None;
    }
}

impl Drop for StagedUpdate {
    fn drop(&mut self) {
        if let Some(directory) = self.cleanup.take() {
            let _ = std::fs::remove_dir_all(directory);
        }
    }
}

/// One release from the update feed, for the modal's Version History. The
/// version and notes are all the history view needs; the download URL and
/// signature stay on the worker that fetched them.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Release {
    pub version: String,
    pub notes: Option<String>,
}

pub struct Updater {
    status: Arc<Mutex<UpdateStatus>>,
    staged: Arc<Mutex<Option<StagedUpdate>>>,
    /// The newest release the last check's feed named, when it is newer than
    /// this build. It carries what a download needs, and the version and notes
    /// the modal shows before anything is fetched.
    offer: Arc<Mutex<Option<ReleaseOffer>>>,
    /// A download is running. Doubles as the "only one at a time" guard.
    downloading: Arc<AtomicBool>,
    /// The running download's cancel request.
    cancel_download: Arc<AtomicBool>,
    /// The feed's releases, newest first: everything Version History shows.
    /// Rewritten by each successful check, so a failed one keeps the last list.
    history: Arc<Mutex<Vec<Release>>>,
    /// A check is running. Separate from `status` because a silent check is
    /// deliberately invisible, so the published status cannot keep two
    /// checks from overlapping.
    checking: Arc<AtomicBool>,
    /// Whether the in-flight check reports its outcome. An explicit request
    /// that lands while a silent one runs sets it rather than being dropped.
    explicit_check: Arc<AtomicBool>,
    automatic: Arc<AtomicBool>,
    preference_path: PathBuf,
    events: mpsc::Sender<UpdaterEvent>,
    receiver: mpsc::Receiver<UpdaterEvent>,
    /// The managed install this build may replace, when there is one. A
    /// forced dev build can check for updates without one (there is simply
    /// nothing to install).
    #[cfg(unix)]
    layout: Option<InstallLayout>,
}

impl Updater {
    /// Load the embedded key and start the updater. Returns `None` when this
    /// build cannot update itself: debug builds unless forced, a bare
    /// `cargo run` binary outside a managed install, or a keyless build.
    pub fn init() -> Option<Self> {
        let forced = std::env::var_os("ORBIT_FORCE_UPDATER").is_some_and(|value| value == "1");
        if cfg!(debug_assertions) && !forced {
            return None;
        }
        if FEED_URL.is_empty() {
            return None;
        }
        if verifying_key().is_none() {
            eprintln!("Orbit updater: the compiled-in public key is not a valid ed25519 key");
            return None;
        }

        // A managed install is what the swap needs; `ORBIT_FORCE_UPDATER=1`
        // keeps the checker alive without one so a dev run can exercise the
        // modal, but installing stays unavailable.
        #[cfg(unix)]
        let layout = match InstallLayout::discover() {
            Some(layout) => Some(layout),
            None if forced => None,
            None => return None,
        };

        let preference_path = preference_path()?;
        let automatic = Arc::new(AtomicBool::new(read_automatic_preference(&preference_path)));
        let (events, receiver) = mpsc::channel();
        let updater = Self {
            status: Arc::new(Mutex::new(UpdateStatus::Idle)),
            staged: Arc::new(Mutex::new(None)),
            offer: Arc::new(Mutex::new(None)),
            downloading: Arc::new(AtomicBool::new(false)),
            cancel_download: Arc::new(AtomicBool::new(false)),
            history: Arc::new(Mutex::new(Vec::new())),
            checking: Arc::new(AtomicBool::new(false)),
            explicit_check: Arc::new(AtomicBool::new(false)),
            automatic,
            preference_path,
            events,
            receiver,
            #[cfg(unix)]
            layout,
        };

        #[cfg(unix)]
        if let Some(layout) = &updater.layout {
            if let Some(error) = take_update_error(layout) {
                let _ = updater.events.send(UpdaterEvent::Failed(error));
            }
        }

        // Sparkle arms a scheduled checker on macOS; here one silent check
        // per launch is the whole schedule.
        if updater.automatically_checks_for_updates() {
            updater.start_check(false);
        }
        Some(updater)
    }

    /// A user-initiated check. Unlike the silent one it reports both
    /// "already current" and failures.
    pub fn check_for_updates(&self) {
        self.start_check(true);
    }

    fn start_check(&self, user_initiated: bool) {
        // A check cannot replace a download's status, and the helper owns the
        // app once an install starts.
        if matches!(
            self.status(),
            UpdateStatus::Downloading | UpdateStatus::Updating
        ) {
            return;
        }
        if self
            .checking
            .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
            .is_err()
        {
            // A silent launch check is in flight. An explicit request adopts
            // it so the menu still gets an answer rather than being dropped.
            if user_initiated {
                self.explicit_check.store(true, Ordering::Relaxed);
            }
            return;
        }
        self.explicit_check.store(user_initiated, Ordering::Relaxed);

        let status = self.status.clone();
        let staged = self.staged.clone();
        let offer = self.offer.clone();
        let history = self.history.clone();
        let checking = self.checking.clone();
        let explicit_check = self.explicit_check.clone();
        let events = self.events.clone();
        let publish_events = events.clone();
        let publish = move |next: UpdateStatus| {
            let mut status = status
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner());
            // A check leaves the status alone while it runs, so most outcomes
            // are not transitions and must not repaint.
            if *status == next {
                return;
            }
            *status = next;
            let _ = publish_events.send(UpdaterEvent::StatusChanged(next));
        };
        let spawned = std::thread::Builder::new()
            .name("orbit-updater-check".into())
            .spawn(move || {
                let outcome = fetch_feed();
                let report = explicit_check.load(Ordering::Relaxed);
                match outcome {
                    Ok(Fetched {
                        offer: newest,
                        history: releases,
                    }) => {
                        *history
                            .lock()
                            .unwrap_or_else(|poisoned| poisoned.into_inner()) = releases;
                        match newest {
                            Some(newest) => {
                                // A payload staged for another release is
                                // stale: drop it (and its directory) before
                                // offering the newer one.
                                let mut staged = staged
                                    .lock()
                                    .unwrap_or_else(|poisoned| poisoned.into_inner());
                                if staged
                                    .as_ref()
                                    .and_then(|update| update.version.as_deref())
                                    .is_some_and(|version| version != newest.version)
                                {
                                    *staged = None;
                                }
                                let ready = staged.is_some();
                                drop(staged);
                                *offer
                                    .lock()
                                    .unwrap_or_else(|poisoned| poisoned.into_inner()) =
                                    Some(newest);
                                // `Available` promises a download; `Ready`
                                // means the payload is already on disk.
                                publish(if ready {
                                    UpdateStatus::Ready
                                } else {
                                    UpdateStatus::Available
                                });
                            }
                            None => {
                                publish(UpdateStatus::Idle);
                                if report {
                                    let _ = events.send(UpdaterEvent::UpToDate);
                                }
                            }
                        }
                    }
                    Err(error) => {
                        publish(UpdateStatus::Idle);
                        if report {
                            let _ = events.send(UpdaterEvent::Failed(format!("{error:#}")));
                        } else {
                            eprintln!("Orbit updater: {error:#}");
                        }
                    }
                }
                checking.store(false, Ordering::Release);
            });
        if spawned.is_err() {
            self.checking.store(false, Ordering::Release);
        }
    }

    /// Start the helper, then wait off the UI thread for its validation
    /// acknowledgement. Only that acknowledgement emits `QuitAndInstall`.
    #[cfg(unix)]
    pub fn install_available_update(&self) -> bool {
        // A check-only build (no managed install) has nothing to swap.
        let Some(layout) = self.layout.as_ref() else {
            return false;
        };
        let update = {
            let mut staged = self
                .staged
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner());
            match staged.take() {
                Some(update) => update,
                None => return false,
            }
        };

        let ready_file = layout.parent.join(format!(
            ".{}.update-ready-{}-{nonce}",
            layout.prefix_name,
            std::process::id(),
            nonce = unique_nonce()
        ));
        let mut command = std::process::Command::new(
            std::env::current_exe().unwrap_or_else(|_| layout.relaunch_executable()),
        );
        command
            .arg(INSTALL_HELPER_FLAG)
            .arg("--install-dir")
            .arg(&layout.install_dir)
            .arg("--staged-dir")
            .arg(update.path())
            .arg("--parent-pid")
            .arg(std::process::id().to_string())
            .arg("--ready-file")
            .arg(&ready_file)
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::inherit());
        let mut child = match command.spawn() {
            Ok(child) => child,
            Err(error) => {
                self.fail_install(tr!("updater.helper_failed", error = error));
                return false;
            }
        };

        self.set_status(UpdateStatus::Updating);

        // A dedicated thread owns the child and the payload, so the staged
        // directory survives until the helper has it and is reclaimed if the
        // helper rejects the handoff.
        let status = self.status.clone();
        let events = self.events.clone();
        // The thread owns the child and the payload. If it cannot start, the
        // closure is dropped along with the staged update (and the orphaned
        // helper times itself out); the app keeps running the current build.
        let spawned = std::thread::Builder::new()
            .name("orbit-updater-handoff".into())
            .spawn(move || {
                use std::io::BufRead as _;
                let mut acknowledgement = String::new();
                let read = child
                    .stdout
                    .take()
                    .map(std::io::BufReader::new)
                    .and_then(|mut stdout| stdout.read_line(&mut acknowledgement).ok());
                if read.is_some() && acknowledgement.trim_end() == "READY" {
                    update.disarm();
                    let _ = events.send(UpdaterEvent::QuitAndInstall);
                    let _ = child.wait();
                    return;
                }

                let _ = child.kill();
                let _ = child.wait();
                *status
                    .lock()
                    .unwrap_or_else(|poisoned| poisoned.into_inner()) = UpdateStatus::Idle;
                let reason = acknowledgement
                    .strip_prefix("ERROR\t")
                    .map(str::trim)
                    .filter(|reason| !reason.is_empty())
                    .unwrap_or("the update helper did not accept the staged build");
                let _ = events.send(UpdaterEvent::Failed(reason.to_owned()));
            });
        if spawned.is_err() {
            self.fail_install("could not start the update handoff worker");
            return false;
        }
        true
    }

    /// Run the staged installer and leave. Inno Setup closes this process,
    /// replaces it in place, and starts the new build.
    #[cfg(windows)]
    pub fn install_available_update(&self) -> bool {
        let update = {
            let mut staged = self
                .staged
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner());
            match staged.take() {
                Some(update) => update,
                None => return false,
            }
        };
        let installer = update.path().to_path_buf();
        let mut command = std::process::Command::new(&installer);
        command.args(["/SILENT", "/NORESTART", "/SP-"]);
        if let Some(directory) = std::env::current_exe()
            .ok()
            .and_then(|exe| exe.parent().map(Path::to_path_buf))
        {
            command.arg(format!("/DIR={}", directory.display()));
        }
        orbit_rpc::hide_console(&mut command);
        match command.spawn() {
            Ok(_) => {
                // The installer owns the file now; leave the staging directory
                // for it rather than deleting the running program.
                update.disarm();
                self.set_status(UpdateStatus::Updating);
                true
            }
            Err(error) => {
                self.fail_install(format!("{installer:?}: {error}"));
                false
            }
        }
    }

    #[cfg(not(any(unix, windows)))]
    pub fn install_available_update(&self) -> bool {
        false
    }

    /// Download and stage the release the last check offered. The bytes only
    /// move when this is called, so the modal has already shown the version
    /// and changelog by the time anything is fetched.
    pub fn download_available_update(&self) -> bool {
        // A check-only build reports a release it has nowhere to install.
        if !self.can_install() {
            return false;
        }
        if self.status() != UpdateStatus::Available {
            return false;
        }
        if self
            .downloading
            .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
            .is_err()
        {
            return false;
        }
        let offered = self
            .offer
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .clone();
        let Some(offer) = offered else {
            self.downloading.store(false, Ordering::Release);
            return false;
        };

        self.cancel_download.store(false, Ordering::Release);
        self.set_status(UpdateStatus::Downloading);

        let status = self.status.clone();
        let staged = self.staged.clone();
        let events = self.events.clone();
        let cancel = self.cancel_download.clone();
        let downloading = self.downloading.clone();
        #[cfg(unix)]
        let layout = self.layout.clone();

        let spawned = std::thread::Builder::new()
            .name("orbit-updater-download".into())
            .spawn(move || {
                let mut throttle = ProgressThrottle::default();
                let progress_events = events.clone();
                let mut progress = move |downloaded: u64, total: u64| {
                    if throttle.should_emit(downloaded, total) {
                        let _ = progress_events.send(UpdaterEvent::Progress { downloaded, total });
                    }
                };
                #[cfg(unix)]
                let outcome = download_and_stage(&offer, &mut progress, &cancel, layout.as_ref());
                #[cfg(not(unix))]
                let outcome = download_and_stage(&offer, &mut progress, &cancel);
                // The offer survives a failure (and a cancel), so Download can
                // be pressed again without another feed request.
                let next = match outcome {
                    Ok(update) => {
                        *staged
                            .lock()
                            .unwrap_or_else(|poisoned| poisoned.into_inner()) = Some(update);
                        UpdateStatus::Ready
                    }
                    Err(error) => {
                        if !cancel.load(Ordering::Relaxed) {
                            let _ = events.send(UpdaterEvent::Failed(format!("{error:#}")));
                        }
                        UpdateStatus::Available
                    }
                };
                *status
                    .lock()
                    .unwrap_or_else(|poisoned| poisoned.into_inner()) = next;
                let _ = events.send(UpdaterEvent::StatusChanged(next));
                downloading.store(false, Ordering::Release);
            });
        if spawned.is_err() {
            self.downloading.store(false, Ordering::Release);
            self.fail_install("could not start the update download");
            return false;
        }
        true
    }

    /// Ask the running download to stop. The offer stays, so Download can be
    /// pressed again.
    pub fn cancel_download(&self) {
        if self.status() == UpdateStatus::Downloading {
            self.cancel_download.store(true, Ordering::Release);
        }
    }

    /// Whether this build can install a release it reports. Forced dev builds
    /// run without a managed install: they check and show the modal, but the
    /// update itself is not offered.
    #[cfg(unix)]
    pub fn can_install(&self) -> bool {
        self.layout.is_some()
    }

    #[cfg(not(unix))]
    pub fn can_install(&self) -> bool {
        cfg!(windows)
    }

    pub fn status(&self) -> UpdateStatus {
        *self
            .status
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    /// The release the modal names: the staged payload's version once it is
    /// on disk, otherwise the offered release's. `None` when there is neither.
    pub fn available_version(&self) -> Option<String> {
        if let Some(version) = self
            .staged
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .as_ref()
            .and_then(|update| update.version.clone())
        {
            return Some(version);
        }
        self.offer
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .as_ref()
            .map(|offer| offer.version.clone())
    }

    /// The release's notes from the feed, for the update modal — readable
    /// before the download, since the check already fetched them. `None` when
    /// no release is on offer or the feed carried no `<description>`.
    pub fn available_notes(&self) -> Option<String> {
        if let Some(notes) = self
            .staged
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .as_ref()
            .and_then(|update| update.notes.clone())
        {
            return Some(notes);
        }
        self.offer
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .as_ref()
            .and_then(|offer| offer.notes.clone())
    }

    /// Every release the last successful check's feed carried, newest first,
    /// for the update modal's Version History. Empty before the first check
    /// answers and after a check that never reached the feed.
    pub fn history(&self) -> Vec<Release> {
        self.history
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .clone()
    }

    /// Drain one pending event. The heartbeat calls this each tick.
    pub fn try_recv_event(&self) -> Option<UpdaterEvent> {
        self.receiver.try_recv().ok()
    }

    pub fn automatically_checks_for_updates(&self) -> bool {
        self.automatic.load(Ordering::Relaxed)
    }

    pub fn set_automatically_checks_for_updates(&self, enabled: bool) {
        if self.automatic.swap(enabled, Ordering::Relaxed) == enabled {
            return;
        }
        let path = self.preference_path.clone();
        // A settings toggle must not wait on the filesystem.
        let _ = std::thread::Builder::new()
            .name("orbit-updater-preference".into())
            .spawn(move || write_automatic_preference(&path, enabled));
        if enabled {
            self.start_check(false);
        }
    }

    fn set_status(&self, next: UpdateStatus) {
        let mut status = self
            .status
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        if *status == next {
            return;
        }
        *status = next;
        let _ = self.events.send(UpdaterEvent::StatusChanged(next));
    }

    fn fail_install(&self, error: impl Into<String>) {
        self.set_status(UpdateStatus::Idle);
        let _ = self.events.send(UpdaterEvent::Failed(error.into()));
    }
}

// ── the unix install seam ───────────────────────────────────────────────

/// A managed, user-writable install this process may replace.
#[cfg(unix)]
#[derive(Clone, Debug)]
struct InstallLayout {
    /// Directory replaced by the update: the `.app` on macOS, the prefix on
    /// Linux.
    install_dir: PathBuf,
    /// Directory the install lives in; staging siblings and the rollback and
    /// ready markers live here, so the swap never crosses filesystems.
    parent: PathBuf,
    prefix_name: String,
    exe_name: String,
}

#[cfg(unix)]
impl InstallLayout {
    fn discover() -> Option<Self> {
        // A root-owned session must not turn the app into a package manager.
        if unsafe { libc::geteuid() } == 0 {
            return None;
        }

        let executable = std::env::current_exe().ok()?.canonicalize().ok()?;
        let exe_name = executable.file_name()?.to_str()?.to_owned();

        #[cfg(target_os = "macos")]
        let install_dir = {
            let macos_dir = executable.parent()?;
            let contents = macos_dir.parent()?;
            let app = contents.parent()?;
            if macos_dir.file_name()? != "MacOS" || contents.file_name()? != "Contents" {
                return None;
            }
            if !app.file_name()?.to_str()?.ends_with(".app") {
                return None;
            }
            app.to_path_buf()
        };

        #[cfg(not(target_os = "macos"))]
        let install_dir = {
            if executable.parent()?.file_name()? != "bin" {
                return None;
            }
            executable.parent()?.parent()?.to_path_buf()
        };

        let install_dir = install_dir.canonicalize().ok()?;
        #[cfg(not(target_os = "macos"))]
        if !linux_replaceable_install(&install_dir) {
            return None;
        }
        let parent = install_dir.parent()?.canonicalize().ok()?;
        let prefix_name = install_dir.file_name()?.to_str()?.to_owned();

        // The swap needs write access to the parent, not merely to files in
        // the current install. Probe once during initialization.
        if !probe_writable(&parent, &prefix_name) {
            return None;
        }

        Some(Self {
            install_dir,
            parent,
            prefix_name,
            exe_name,
        })
    }

    fn relaunch_executable(&self) -> PathBuf {
        relaunch_executable(&self.install_dir, &self.exe_name)
    }

    fn error_path(&self) -> PathBuf {
        self.parent
            .join(format!(".{}.update-error", self.prefix_name))
    }
}

/// Linux tarballs are dedicated directories, not generic filesystem prefixes.
/// Reject unknown entries and symlinks, including in legacy unmarked releases.
#[cfg(all(unix, any(not(target_os = "macos"), test)))]
fn linux_replaceable_install(install: &Path) -> bool {
    const DIRS: &[&str] = &[
        "bin",
        "share",
        "share/applications",
        "share/icons",
        "share/icons/hicolor",
        "share/icons/hicolor/256x256",
        "share/icons/hicolor/256x256/apps",
        "share/licenses",
        "share/licenses/orbit-pi",
    ];
    const FILES: &[&str] = &[
        "bin/orbit-pi",
        "share/applications/dev.orbit.pi.desktop",
        "share/icons/hicolor/256x256/apps/dev.orbit.pi.png",
        "share/licenses/orbit-pi/LICENSE",
    ];
    let Some(name) = install.file_name().and_then(|n| n.to_str()) else {
        return false;
    };
    if !(name == "orbit-pi" || name.starts_with("orbit-pi-"))
        || install.canonicalize().ok().as_deref() != Some(install)
    {
        return false;
    }
    fn check(root: &Path, relative: &Path, dirs: &[&str], files: &[&str]) -> bool {
        let Ok(entries) = std::fs::read_dir(root.join(relative)) else {
            return false;
        };
        for entry in entries {
            let Ok(entry) = entry else { return false };
            let path = relative.join(entry.file_name());
            let Some(name) = path.to_str() else {
                return false;
            };
            let Ok(kind) = entry.file_type() else {
                return false;
            };
            if kind.is_dir() && dirs.contains(&name) {
                if !check(root, &path, dirs, files) {
                    return false;
                }
            } else if !(kind.is_file() && files.contains(&name)) {
                return false;
            }
        }
        true
    }
    install.join("bin/orbit-pi").is_file() && check(install, Path::new(""), DIRS, FILES)
}

/// The executable to relaunch inside an installed tree.
#[cfg(unix)]
fn relaunch_executable(install_dir: &Path, exe_name: &str) -> PathBuf {
    #[cfg(target_os = "macos")]
    {
        install_dir.join("Contents").join("MacOS").join(exe_name)
    }
    #[cfg(not(target_os = "macos"))]
    {
        install_dir.join("bin").join(exe_name)
    }
}

#[cfg(unix)]
fn probe_writable(parent: &Path, prefix_name: &str) -> bool {
    let candidate = parent.join(format!(
        ".{prefix_name}.update-probe-{}-{}",
        std::process::id(),
        unique_nonce()
    ));
    match std::fs::create_dir(&candidate) {
        Ok(()) => {
            let _ = std::fs::remove_dir(&candidate);
            true
        }
        Err(_) => false,
    }
}

/// A hidden helper invocation, run before the UI starts. Returns the process
/// exit code when the arguments name an install handoff.
#[cfg(unix)]
pub fn run_install_helper() -> Option<i32> {
    let args: Vec<std::ffi::OsString> = std::env::args_os().collect();
    let index = args.iter().position(|arg| arg == INSTALL_HELPER_FLAG)?;
    // A malformed helper invocation must not fall through to a second UI.
    let Some(flags) = HelperFlags::parse(&args[index + 1..]) else {
        eprintln!("Orbit updater: malformed install-helper arguments");
        return Some(1);
    };
    Some(run_install(&flags))
}

#[cfg(not(unix))]
pub fn run_install_helper() -> Option<i32> {
    None
}

#[cfg(unix)]
struct HelperFlags {
    install_dir: PathBuf,
    staged_dir: PathBuf,
    parent_pid: u32,
    ready_file: PathBuf,
}

#[cfg(unix)]
impl HelperFlags {
    fn parse(args: &[std::ffi::OsString]) -> Option<Self> {
        let mut install_dir = None;
        let mut staged_dir = None;
        let mut parent_pid = None;
        let mut ready_file = None;
        let mut iter = args.iter();
        while let Some(flag) = iter.next() {
            let value = iter.next()?;
            match flag.to_str()? {
                "--install-dir" => install_dir = Some(PathBuf::from(value)),
                "--staged-dir" => staged_dir = Some(PathBuf::from(value)),
                "--parent-pid" => parent_pid = value.to_str()?.parse().ok(),
                "--ready-file" => ready_file = Some(PathBuf::from(value)),
                _ => return None,
            }
        }
        Some(Self {
            install_dir: install_dir?,
            staged_dir: staged_dir?,
            parent_pid: parent_pid?,
            ready_file: ready_file?,
        })
    }
}

#[cfg(unix)]
fn run_install(flags: &HelperFlags) -> i32 {
    let running_exe = match std::env::current_exe().and_then(std::fs::File::open) {
        Ok(file) => file,
        Err(_) => return helper_error(flags, "the running executable is unavailable"),
    };
    let exe_name = match std::env::current_exe().ok().and_then(|exe| {
        exe.file_name()
            .map(|name| name.to_string_lossy().into_owned())
    }) {
        Some(name) => name,
        None => return helper_error(flags, "the running executable has no name"),
    };

    if !flags.install_dir.is_dir() {
        return helper_error(flags, "the install directory is missing");
    }
    #[cfg(not(target_os = "macos"))]
    if unsafe { libc::geteuid() } == 0
        || !linux_replaceable_install(&flags.install_dir)
        || std::env::current_exe()
            .ok()
            .and_then(|p| p.canonicalize().ok())
            != flags.install_dir.join("bin/orbit-pi").canonicalize().ok()
    {
        return helper_error(
            flags,
            "the install directory is not a dedicated Orbit install",
        );
    }
    // Only ever swap a namespaced staging sibling of this install, whose new
    // executable really is executable. The staged directory is the extracted
    // release with its archive root stripped, so it *is* the new install's
    // contents.
    if !is_staging_sibling(&flags.install_dir, &flags.staged_dir) {
        return helper_error(flags, "the staged update is not a sibling of the install");
    }
    if !flags.staged_dir.is_dir() {
        return helper_error(flags, "the staged update is missing");
    }
    // The helper is spawned directly by the app, so its real parent must be
    // the process that asked for the swap.
    if unsafe { libc::getppid() as u32 } != flags.parent_pid {
        return helper_error(flags, "the update helper was not launched by the app");
    }
    use std::os::unix::fs::PermissionsExt as _;
    match std::fs::metadata(staged_executable(&flags.staged_dir, &exe_name)) {
        Ok(metadata) if metadata.is_file() && metadata.permissions().mode() & 0o111 != 0 => {}
        _ => return helper_error(flags, "the staged update is missing its executable"),
    }

    // Acknowledge the handoff before waiting, so the app can run its normal
    // quit handlers and release the filesystem.
    println!("READY");
    use std::io::Write as _;
    let _ = std::io::stdout().flush();

    if !wait_for_exit(flags.parent_pid, PARENT_EXIT_TIMEOUT) {
        return helper_error(flags, "the app did not quit in time");
    }

    // Hold the directory lock through the swap and any rollback. Unlike a
    // lock file, the directory cannot be replaced without moving the install.
    let _lock = match lock_update_parent(flags.parent_dir()) {
        Ok(lock) => lock,
        Err(_) => return relaunch_after_error(flags, &exe_name, "the update lock is unavailable"),
    };
    if !same_executable(
        &running_exe,
        &relaunch_executable(&flags.install_dir, &exe_name),
    ) {
        return relaunch_after_error(flags, &exe_name, "the install changed during handoff");
    }

    let prefix_name = flags
        .install_dir
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_else(|| "Orbit".into());
    let backup = flags.parent_dir().join(format!(
        ".{prefix_name}.rollback-{}-{}",
        std::process::id(),
        unique_nonce()
    ));
    #[cfg(target_os = "macos")]
    let _ = std::fs::remove_dir_all(&backup);
    #[cfg(not(target_os = "macos"))]
    {
        // Never remove a pre-existing path, even one with our generated name.
        match backup.symlink_metadata() {
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            _ => return relaunch_after_error(flags, &exe_name, "the rollback path is unavailable"),
        }
        if !linux_replaceable_install(&flags.install_dir) {
            return relaunch_after_error(
                flags,
                &exe_name,
                "the install directory changed during handoff",
            );
        }
    }

    if let Err(error) = std::fs::rename(&flags.install_dir, &backup) {
        let _ = spawn_install(&flags.install_dir, &exe_name, None);
        return helper_error(flags, &tr!("updater.move_aside_failed", error = error));
    }
    sync_directory(flags.parent_dir());

    if let Err(error) = std::fs::rename(&flags.staged_dir, &flags.install_dir) {
        let _ = std::fs::rename(&backup, &flags.install_dir);
        sync_directory(flags.parent_dir());
        let _ = spawn_install(&flags.install_dir, &exe_name, None);
        return helper_error(flags, &tr!("updater.move_into_place_failed", error = error));
    }
    sync_directory(flags.parent_dir());

    let mut child = match spawn_install(&flags.install_dir, &exe_name, Some(&flags.ready_file)) {
        Ok(child) => child,
        Err(error) => {
            #[cfg(target_os = "macos")]
            let _ = std::fs::remove_dir_all(&flags.install_dir);
            #[cfg(not(target_os = "macos"))]
            let _ = std::fs::rename(&flags.install_dir, &flags.staged_dir);
            let _ = std::fs::rename(&backup, &flags.install_dir);
            sync_directory(flags.parent_dir());
            let _ = spawn_install(&flags.install_dir, &exe_name, None);
            return helper_error(flags, &tr!("updater.relaunch_failed", error = error));
        }
    };

    match wait_for_relaunch(&mut child, &flags.ready_file, RELAUNCH_READY_TIMEOUT) {
        RelaunchState::Ready => {
            // Do not delete an untrusted helper argument on Linux.
            #[cfg(target_os = "macos")]
            let _ = std::fs::remove_file(&flags.ready_file);
            // Even release paths can contain user-modified files.
            #[cfg(target_os = "macos")]
            let _ = std::fs::remove_dir_all(&backup);
            sync_directory(flags.parent_dir());
            0
        }
        RelaunchState::Exited(status) => {
            // The replacement died before it opened a window. Put the old
            // build back and relaunch it, so a bad release cannot strand the
            // user.
            #[cfg(target_os = "macos")]
            let _ = std::fs::remove_dir_all(&flags.install_dir);
            #[cfg(not(target_os = "macos"))]
            let _ = std::fs::rename(&flags.install_dir, &flags.staged_dir);
            let _ = std::fs::rename(&backup, &flags.install_dir);
            sync_directory(flags.parent_dir());
            let _ = spawn_install(&flags.install_dir, &exe_name, None);
            helper_error(
                flags,
                &tr!("updater.new_build_exited", status = status.to_string()),
            )
        }
        RelaunchState::TimedOut => {
            // Never yank a process that may still be starting. Keep the new
            // build in place and retain its rollback copy for manual
            // recovery rather than destructively guessing.
            eprintln!(
                "Orbit updater: the new build stayed alive but did not acknowledge startup; retaining {}",
                backup.display()
            );
            0
        }
    }
}

#[cfg(unix)]
fn lock_update_parent(parent: &Path) -> std::io::Result<std::fs::File> {
    use std::os::unix::io::AsRawFd as _;
    let directory = std::fs::File::open(parent)?;
    loop {
        if unsafe { libc::flock(directory.as_raw_fd(), libc::LOCK_EX) } == 0 {
            return Ok(directory);
        }
        let error = std::io::Error::last_os_error();
        if error.kind() != std::io::ErrorKind::Interrupted {
            return Err(error);
        }
    }
}

#[cfg(unix)]
fn same_executable(running: &std::fs::File, installed: &Path) -> bool {
    use std::os::unix::fs::MetadataExt as _;
    match (running.metadata(), std::fs::metadata(installed)) {
        (Ok(a), Ok(b)) => a.is_file() && b.is_file() && a.dev() == b.dev() && a.ino() == b.ino(),
        _ => false,
    }
}

#[cfg(unix)]
fn relaunch_after_error(flags: &HelperFlags, exe_name: &str, reason: &str) -> i32 {
    let result = helper_error(flags, reason);
    let _ = spawn_install(&flags.install_dir, exe_name, None);
    result
}

/// Relaunch an installed build, optionally arming the startup handshake.
#[cfg(unix)]
fn spawn_install(
    install_dir: &Path,
    exe_name: &str,
    ready_file: Option<&Path>,
) -> std::io::Result<std::process::Child> {
    let mut command = std::process::Command::new(relaunch_executable(install_dir, exe_name));
    command.stdin(std::process::Stdio::null());
    if let Some(ready_file) = ready_file {
        command.env(RELAUNCH_READY_ENV, ready_file);
    }
    command.spawn()
}

/// How the relaunched build resolved the startup handshake.
#[cfg(unix)]
enum RelaunchState {
    Ready,
    Exited(std::process::ExitStatus),
    TimedOut,
}

#[cfg(unix)]
impl HelperFlags {
    fn parent_dir(&self) -> &Path {
        self.install_dir.parent().unwrap_or(Path::new("."))
    }
}

#[cfg(unix)]
const PARENT_EXIT_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(300);
#[cfg(unix)]
const RELAUNCH_READY_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(30);
#[cfg(unix)]
const POLL_INTERVAL: std::time::Duration = std::time::Duration::from_millis(200);

#[cfg(unix)]
fn wait_for_exit(pid: u32, timeout: std::time::Duration) -> bool {
    let deadline = std::time::Instant::now() + timeout;
    while process_alive(pid) {
        if std::time::Instant::now() >= deadline {
            return false;
        }
        std::thread::sleep(POLL_INTERVAL);
    }
    true
}

#[cfg(unix)]
fn process_alive(pid: u32) -> bool {
    let result = unsafe { libc::kill(pid as libc::pid_t, 0) };
    if result == 0 {
        return true;
    }
    std::io::Error::last_os_error().raw_os_error() != Some(libc::ESRCH)
}

#[cfg(unix)]
fn wait_for_relaunch(
    child: &mut std::process::Child,
    ready_file: &Path,
    timeout: std::time::Duration,
) -> RelaunchState {
    let deadline = std::time::Instant::now() + timeout;
    loop {
        if ready_file.exists() {
            return RelaunchState::Ready;
        }
        match child.try_wait() {
            Ok(Some(status)) => return RelaunchState::Exited(status),
            Ok(None) => {}
            Err(error) => {
                eprintln!("Orbit updater: could not observe the relaunched app: {error}");
                return RelaunchState::TimedOut;
            }
        }
        if std::time::Instant::now() >= deadline {
            return RelaunchState::TimedOut;
        }
        std::thread::sleep(POLL_INTERVAL);
    }
}

/// Flush a directory's entries so a completed rename survives a crash. Its
/// failure is not fatal: the swap already happened, only durability is lost.
#[cfg(unix)]
fn sync_directory(directory: &Path) {
    let _ = std::fs::File::open(directory).and_then(|directory| directory.sync_all());
}

/// The main executable inside an install's *contents* (an extracted archive
/// with its root stripped, or a live install directory).
#[cfg(unix)]
fn staged_executable(install_contents: &Path, exe_name: &str) -> PathBuf {
    #[cfg(target_os = "macos")]
    {
        install_contents
            .join("Contents")
            .join("MacOS")
            .join(exe_name)
    }
    #[cfg(not(target_os = "macos"))]
    {
        install_contents.join("bin").join(exe_name)
    }
}

/// Whether `staged_dir` is a namespaced staging sibling of `install_dir`: the
/// only shape the helper will swap in. Everything else — an arbitrary
/// directory, a path from another prefix, or the install itself — is rejected
/// before anything is renamed.
#[cfg(unix)]
fn is_staging_sibling(install_dir: &Path, staged_dir: &Path) -> bool {
    let Some(parent) = install_dir.parent() else {
        return false;
    };
    let Some(prefix_name) = install_dir.file_name().and_then(|name| name.to_str()) else {
        return false;
    };
    staged_dir.parent() == Some(parent)
        && staged_dir
            .file_name()
            .and_then(|name| name.to_str())
            .is_some_and(|name| name.starts_with(&format!(".{prefix_name}.update-")))
}

#[cfg(unix)]
fn helper_error(flags: &HelperFlags, reason: &str) -> i32 {
    eprintln!("Orbit updater: {reason}");
    let prefix_name = flags
        .install_dir
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_else(|| "Orbit".into());
    let path = flags
        .parent_dir()
        .join(format!(".{prefix_name}.update-error"));
    #[cfg(target_os = "macos")]
    let _ = std::fs::write(path, format!("{reason}\n"));
    #[cfg(not(target_os = "macos"))]
    {
        use std::io::Write as _;
        if let Ok(mut file) = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(path)
        {
            let _ = writeln!(file, "{reason}");
        }
    }
    println!("ERROR\t{reason}");
    1
}

/// Write the readiness marker the helper waits on. The path comes from the
/// helper through the environment, but is only honored when it sits beside
/// this install and names it, so the environment cannot redirect the write.
#[cfg(unix)]
pub(crate) fn signal_relaunch_ready() {
    let Some(path) = std::env::var_os(RELAUNCH_READY_ENV).map(PathBuf::from) else {
        return;
    };
    let Some((parent, prefix_name)) = current_ready_scope() else {
        return;
    };
    if path.parent() != Some(parent.as_path()) {
        return;
    }
    let Some(name) = path.file_name().and_then(|name| name.to_str()) else {
        return;
    };
    if !name.starts_with(&format!(".{prefix_name}.update-ready-")) {
        return;
    }
    let _ = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&path)
        .and_then(|mut file| {
            use std::io::Write as _;
            file.write_all(b"ready\n")
        });
}

#[cfg(not(unix))]
pub(crate) fn signal_relaunch_ready() {}

#[cfg(unix)]
fn current_ready_scope() -> Option<(PathBuf, String)> {
    let executable = std::env::current_exe().ok()?.canonicalize().ok()?;
    let install_dir = install_dir_for(&executable)?;
    let parent = install_dir.parent()?.canonicalize().ok()?;
    let prefix_name = install_dir.file_name()?.to_str()?.to_owned();
    Some((parent, prefix_name))
}

/// The install directory a given executable lives in, or `None` when the
/// executable is not inside a managed install.
#[cfg(unix)]
fn install_dir_for(executable: &Path) -> Option<PathBuf> {
    #[cfg(target_os = "macos")]
    {
        let macos_dir = executable.parent()?;
        let contents = macos_dir.parent()?;
        let app = contents.parent()?;
        if macos_dir.file_name()? != "MacOS" || contents.file_name()? != "Contents" {
            return None;
        }
        if !app.file_name()?.to_str()?.ends_with(".app") {
            return None;
        }
        Some(app.to_path_buf())
    }
    #[cfg(not(target_os = "macos"))]
    {
        if executable.parent()?.file_name()? != "bin" {
            return None;
        }
        Some(executable.parent()?.parent()?.to_path_buf())
    }
}

// ── staging ─────────────────────────────────────────────────────────────

/// What one check found: the release to offer, when the feed names a newer
/// one, and the feed's full release list for Version History.
struct Fetched {
    offer: Option<ReleaseOffer>,
    history: Vec<Release>,
}

/// The newest release the feed names, complete enough to download later. A
/// check keeps this and nothing else, so version and changelog can be shown
/// before a single byte of the artifact is fetched.
#[derive(Clone, Debug)]
struct ReleaseOffer {
    version: String,
    notes: Option<String>,
    url: String,
    signature: String,
    length: Option<u64>,
}

/// Read the feed. This is the whole cost of a check — one small request — so
/// a silent launch check never downloads a release the user has not seen.
fn fetch_feed() -> anyhow::Result<Fetched> {
    let document = http_get(FEED_URL)?;
    let items = feed::items(&document);
    let history = items
        .iter()
        .map(|item| Release {
            version: item.version.clone(),
            notes: item.notes.clone(),
        })
        .collect();
    let Some(item) = items.into_iter().next() else {
        anyhow::bail!("the update feed has no signed release");
    };
    if !feed::is_newer(&item.version, env!("CARGO_PKG_VERSION")) {
        return Ok(Fetched {
            offer: None,
            history,
        });
    }
    Ok(Fetched {
        offer: Some(ReleaseOffer {
            version: item.version,
            notes: item.notes,
            url: item.url,
            signature: item.signature,
            length: item.length,
        }),
        history,
    })
}

/// Download the offered artifact, verify it against the release signature,
/// and stage it for the platform's install seam. `progress` receives
/// (downloaded, total) bytes as they arrive; `cancel` stops between chunks.
#[cfg(unix)]
fn download_and_stage(
    offer: &ReleaseOffer,
    progress: &mut dyn FnMut(u64, u64),
    cancel: &AtomicBool,
    layout: Option<&InstallLayout>,
) -> anyhow::Result<StagedUpdate> {
    // A check-only build has nowhere to put a payload, so it never offers one.
    let Some(layout) = layout else {
        anyhow::bail!("this build cannot install an update");
    };
    let (archive, temporary) = fetch_artifact(offer, progress, cancel)?;
    let mut staged = stage_artifact(archive, temporary, &offer.version, layout)?
        .ok_or_else(|| anyhow::anyhow!("the update could not be staged"))?;
    staged.notes = offer.notes.clone();
    staged.version = Some(offer.version.clone());
    Ok(staged)
}

#[cfg(not(unix))]
fn download_and_stage(
    offer: &ReleaseOffer,
    progress: &mut dyn FnMut(u64, u64),
    cancel: &AtomicBool,
) -> anyhow::Result<StagedUpdate> {
    let (archive, temporary) = fetch_artifact(offer, progress, cancel)?;
    let mut staged = stage_artifact(archive, temporary)?
        .ok_or_else(|| anyhow::anyhow!("the update could not be staged"))?;
    staged.notes = offer.notes.clone();
    staged.version = Some(offer.version.clone());
    Ok(staged)
}

/// Validate the offer, download the artifact into a temporary directory, and
/// check both its declared length and its release signature. Returns the
/// archive and the directory that owns it.
fn fetch_artifact(
    offer: &ReleaseOffer,
    progress: &mut dyn FnMut(u64, u64),
    cancel: &AtomicBool,
) -> anyhow::Result<(PathBuf, PathBuf)> {
    validate_release_version(&offer.version)?;
    validate_download_url(&offer.url)?;
    let declared_length = offer
        .length
        .filter(|length| *length > 0 && *length <= MAX_ARTIFACT_BYTES)
        .ok_or_else(|| {
            anyhow::anyhow!("the update feed does not declare a valid artifact length")
        })?;

    let directory = unique_temp_directory("update")?;
    let archive = directory.join(artifact_name());
    let verified = download_to(
        &offer.url,
        &archive,
        600,
        MAX_ARTIFACT_BYTES,
        declared_length,
        progress,
        cancel,
    )
    .and_then(|()| {
        let actual_length = std::fs::metadata(&archive)?.len();
        anyhow::ensure!(
            actual_length == declared_length,
            "the update artifact does not match the length the feed declared"
        );
        verify_artifact(&archive, &offer.signature)
    });
    if let Err(error) = verified {
        let _ = std::fs::remove_dir_all(&directory);
        return Err(error);
    }
    Ok((archive, directory))
}

#[cfg(unix)]
fn artifact_name() -> &'static str {
    "update.tar.gz"
}

#[cfg(windows)]
fn artifact_name() -> &'static str {
    "Orbit-Setup.exe"
}

#[cfg(not(any(unix, windows)))]
fn artifact_name() -> &'static str {
    "update.bin"
}

/// Extract the verified archive into a staging directory beside the install,
/// so the helper's swap is a same-filesystem rename.
#[cfg(unix)]
fn stage_artifact(
    archive: PathBuf,
    temporary: PathBuf,
    version: &str,
    layout: &InstallLayout,
) -> anyhow::Result<Option<StagedUpdate>> {
    let staging = create_unique_sibling(layout, "update")?;
    let expected_root = expected_archive_root(layout, version);
    let extracted = extract_release_archive(&archive, &staging, &expected_root).and_then(|_| {
        anyhow::ensure!(
            staged_executable(&staging, &layout.exe_name).is_file(),
            "the update archive is missing its executable"
        );
        Ok(())
    });
    let _ = std::fs::remove_dir_all(&temporary);
    if let Err(error) = extracted {
        let _ = std::fs::remove_dir_all(&staging);
        return Err(error);
    }
    Ok(Some(StagedUpdate::new(staging.clone(), staging)))
}

/// The single top-level directory a release archive must carry: the `.app`
/// bundle name on macOS, the versioned package name on Linux. Anything else
/// means the feed served an archive for a different build.
#[cfg(unix)]
fn expected_archive_root(layout: &InstallLayout, version: &str) -> std::ffi::OsString {
    #[cfg(target_os = "macos")]
    {
        let _ = version;
        layout
            .install_dir
            .file_name()
            .map(std::ffi::OsStr::to_os_string)
            .unwrap_or_else(|| std::ffi::OsString::from("Orbit Pi.app"))
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = layout;
        std::ffi::OsString::from(format!("orbit-pi-{version}-{}", target_triple()))
    }
}

/// The host triple `scripts/bundle-linux.sh` packages releases under.
#[cfg(all(target_os = "linux", target_arch = "aarch64"))]
fn target_triple() -> &'static str {
    "aarch64-unknown-linux-gnu"
}

#[cfg(all(target_os = "linux", not(target_arch = "aarch64")))]
fn target_triple() -> &'static str {
    "x86_64-unknown-linux-gnu"
}

#[cfg(all(unix, not(target_os = "linux"), not(target_os = "macos")))]
fn target_triple() -> &'static str {
    "unknown"
}

#[cfg(windows)]
fn stage_artifact(archive: PathBuf, directory: PathBuf) -> anyhow::Result<Option<StagedUpdate>> {
    Ok(Some(StagedUpdate::new(archive, directory)))
}

#[cfg(not(any(unix, windows)))]
fn stage_artifact(_archive: PathBuf, _directory: PathBuf) -> anyhow::Result<Option<StagedUpdate>> {
    Ok(None)
}

/// Reserve an unused sibling of the install for staging or probing.
#[cfg(unix)]
fn create_unique_sibling(layout: &InstallLayout, kind: &str) -> anyhow::Result<PathBuf> {
    for _ in 0..100 {
        let candidate = layout.parent.join(format!(
            ".{}.{kind}-{}-{}",
            layout.prefix_name,
            std::process::id(),
            unique_nonce()
        ));
        match std::fs::create_dir(&candidate) {
            Ok(()) => return Ok(candidate),
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(error) => return Err(error.into()),
        }
    }
    anyhow::bail!("could not reserve an update staging directory")
}

/// Extract a signed release archive, stripping its single top-level directory
/// and refusing anything that could escape the staging tree. The top-level
/// entry must match `expected_root`, so an archive built for a different
/// release or platform is rejected before anything is written.
#[cfg(unix)]
fn extract_release_archive(
    path: &Path,
    destination: &Path,
    expected_root: &std::ffi::OsStr,
) -> anyhow::Result<()> {
    use std::collections::HashSet;

    let decoder = flate2::read::GzDecoder::new(std::fs::File::open(path)?);
    let mut archive = tar::Archive::new(decoder);
    let mut saw_root = false;
    let mut seen = HashSet::new();
    let mut unpacked_bytes = 0_u64;
    let mut entry_count = 0_usize;

    for entry in archive.entries()? {
        let mut entry = entry?;
        entry_count += 1;
        anyhow::ensure!(
            entry_count <= MAX_ARCHIVE_ENTRIES,
            "the update archive contains too many entries"
        );

        let archived_path = entry.path()?.into_owned();
        let mut components = archived_path.components();
        let top = match components.next() {
            Some(std::path::Component::Normal(top)) => top,
            _ => anyhow::bail!("the update archive contains an invalid path"),
        };
        anyhow::ensure!(
            top == expected_root,
            "the update archive has an unexpected top-level directory"
        );
        saw_root = true;

        let mut relative = PathBuf::new();
        for component in components {
            match component {
                std::path::Component::Normal(component) => relative.push(component),
                _ => anyhow::bail!("the update archive contains an unsafe path"),
            }
        }
        if relative.as_os_str().is_empty() {
            anyhow::ensure!(
                entry.header().entry_type().is_dir(),
                "the update archive root is not a directory"
            );
            continue;
        }
        anyhow::ensure!(
            seen.insert(relative.clone()),
            "the update archive contains a duplicate path"
        );
        let entry_type = entry.header().entry_type();
        anyhow::ensure!(
            entry_type.is_file() || entry_type.is_dir(),
            "the update archive contains a link or special file"
        );
        unpacked_bytes = unpacked_bytes
            .checked_add(entry.header().size()?)
            .ok_or_else(|| anyhow::anyhow!("the update archive size overflowed"))?;
        anyhow::ensure!(
            unpacked_bytes <= MAX_UNPACKED_BYTES,
            "the update archive expands beyond the safety limit"
        );

        let output = destination.join(relative);
        // Tarballs carry an explicit entry for every directory. Creating a
        // file for one would turn `Contents/` into a regular file and the
        // next entry beneath it would fail with `EEXIST`, so directory
        // entries must stay directories.
        if entry_type.is_dir() {
            std::fs::create_dir_all(&output)?;
            continue;
        }
        if let Some(parent) = output.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let mut file = std::fs::File::create(&output)?;
        std::io::copy(&mut entry, &mut file)?;
        // Preserve the release's mode; `File::create` drops the executable
        // bit the swapped-in build needs to relaunch.
        use std::os::unix::fs::PermissionsExt as _;
        let mode = entry.header().mode()? & 0o7777;
        std::fs::set_permissions(&output, std::fs::Permissions::from_mode(mode))?;
    }

    anyhow::ensure!(saw_root, "the update archive is empty");
    Ok(())
}

fn verify_artifact(path: &Path, signature: &str) -> anyhow::Result<()> {
    let key = verifying_key().ok_or_else(|| anyhow::anyhow!("the public key is unusable"))?;
    let signature = base64::engine::general_purpose::STANDARD
        .decode(signature.trim())
        .ok()
        .and_then(|bytes| <[u8; 64]>::try_from(bytes).ok())
        .map(|bytes| ed25519_dalek::Signature::from_bytes(&bytes))
        .ok_or_else(|| anyhow::anyhow!("the update signature is malformed"))?;
    let bytes = std::fs::read(path)?;
    key.verify_strict(&bytes, &signature)
        .map_err(|_| anyhow::anyhow!("the update artifact failed signature verification"))
}

fn verifying_key() -> Option<ed25519_dalek::VerifyingKey> {
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(PUBLIC_ED_KEY.trim())
        .ok()?;
    ed25519_dalek::VerifyingKey::from_bytes(&<[u8; 32]>::try_from(bytes).ok()?).ok()
}

fn validate_release_version(version: &str) -> anyhow::Result<()> {
    anyhow::ensure!(
        !version.is_empty()
            && version.len() <= 64
            && version.bytes().all(|byte| {
                byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'-' | b'+' | b'_')
            }),
        "the update feed contains an invalid version"
    );
    Ok(())
}

fn validate_download_url(value: &str) -> anyhow::Result<()> {
    let url = url::Url::parse(value)?;
    anyhow::ensure!(
        url.scheme() == "https",
        "the update feed points at a non-https url"
    );
    Ok(())
}

// ── http ────────────────────────────────────────────────────────────────

fn http_client(timeout_seconds: u64) -> anyhow::Result<reqwest::blocking::Client> {
    Ok(reqwest::blocking::Client::builder()
        .timeout(std::time::Duration::from_secs(timeout_seconds))
        // A host can advertise an IPv6 address that blackholes. Without a
        // per-attempt cap the connector waits there instead of falling back
        // to IPv4 — GitHub's release-asset CDN does exactly this on some
        // networks.
        .connect_timeout(std::time::Duration::from_secs(5))
        .user_agent(concat!("Orbit/", env!("CARGO_PKG_VERSION")))
        .build()?)
}

fn http_get(url: &str) -> anyhow::Result<String> {
    // The feed is a few kilobytes, but GitHub's release-asset CDN can be slow
    // to redirect and cold-start; the check is interactive yet patient.
    let response = http_client(60)?
        .get(url)
        .send()
        .map_err(http_failure)?
        .error_for_status()
        .map_err(http_failure)?;
    let bytes = response.bytes().map_err(http_failure)?;
    anyhow::ensure!(
        bytes.len() as u64 <= MAX_FEED_BYTES,
        "the update feed is implausibly large"
    );
    Ok(String::from_utf8(bytes.to_vec())?)
}

fn download_to(
    url: &str,
    destination: &Path,
    timeout_seconds: u64,
    limit: u64,
    // The feed's declared artifact length, used when the host answers without
    // a `Content-Length` (a chunked CDN response does exactly that).
    expected_total: u64,
    progress: &mut dyn FnMut(u64, u64),
    cancel: &AtomicBool,
) -> anyhow::Result<()> {
    use std::io::Read as _;
    use std::io::Write as _;

    let mut response = http_client(timeout_seconds)?
        .get(url)
        .send()
        .map_err(http_failure)?
        .error_for_status()
        .map_err(http_failure)?;
    let total = response
        .content_length()
        .filter(|length| *length > 0)
        .unwrap_or(expected_total);
    let mut file = std::fs::File::create(destination)?;
    let mut buffer = vec![0_u8; 64 * 1024];
    let mut copied = 0_u64;
    progress(0, total);
    loop {
        // A cancel is only honored between chunks, so it can never leave a
        // half-written file behind for the verifier to read.
        anyhow::ensure!(
            !cancel.load(Ordering::Relaxed),
            "the update download was cancelled"
        );
        let read = response.read(&mut buffer)?;
        if read == 0 {
            break;
        }
        copied += read as u64;
        anyhow::ensure!(
            copied <= limit,
            "the downloaded update exceeds its safety limit"
        );
        file.write_all(&buffer[..read])?;
        progress(copied, total);
    }
    file.sync_all()?;
    progress(copied, total);
    Ok(())
}

/// Rate-limit the progress events a download emits: one per whole percent (or
/// per 250 KB on a very large artifact), plus one final tick. A 40 MB download
/// would otherwise send hundreds of events and repaint the window for each.
#[derive(Default)]
struct ProgressThrottle {
    last_percent: Option<u64>,
    last_bytes: u64,
}

impl ProgressThrottle {
    const BYTE_STEP: u64 = 250 * 1024;

    fn should_emit(&mut self, downloaded: u64, total: u64) -> bool {
        let complete = total != 0 && downloaded >= total;
        let percent = (total != 0).then(|| downloaded.saturating_mul(100) / total);
        let emit = complete
            || match percent {
                Some(percent) => self.last_percent != Some(percent),
                // Without a declared total there is no percentage to compare,
                // so fall back to a byte step and let the bar creep forward.
                None => downloaded.saturating_sub(self.last_bytes) >= Self::BYTE_STEP,
            };
        if emit {
            self.last_percent = percent.or(self.last_percent);
            self.last_bytes = downloaded;
        }
        emit
    }
}

/// The release page carrying a release's changelog — the same notes the feed
/// embeds as each item's `<description>`. Derived from the feed's own
/// repository, so a build pointed at a fork links there, and falling back to
/// the crate repository when the feed is not a GitHub release feed.
pub fn changelog_url(version: &str) -> Option<String> {
    if validate_release_version(version).is_err() {
        return None;
    }
    let base = release_repository().unwrap_or_else(|| {
        env!("CARGO_PKG_REPOSITORY")
            .trim_end_matches('/')
            .to_owned()
    });
    Some(format!("{base}/releases/tag/v{version}"))
}

/// `https://github.com/owner/repo` for the feed this build reads, when that
/// feed is one of GitHub's release feeds.
fn release_repository() -> Option<String> {
    let (base, _) = FEED_URL.split_once("/releases/latest/download/")?;
    let url = url::Url::parse(base).ok()?;
    (url.host_str() == Some("github.com")).then(|| base.trim_end_matches('/').to_owned())
}

/// Prefer the rustls/IO cause over reqwest's "error sending request for url"
/// wrapper. The URL is already known, and a truncated banner otherwise hides
/// whether this was TLS, DNS, or a blocked socket.
fn http_failure(error: reqwest::Error) -> anyhow::Error {
    anyhow::anyhow!("{}", deepest_cause(&error))
}

fn deepest_cause(error: &dyn std::error::Error) -> String {
    let mut cause = error.to_string();
    let mut current = error.source();
    while let Some(err) = current {
        cause = err.to_string();
        current = err.source();
    }
    cause
}

// ── preferences and errors ──────────────────────────────────────────────

fn preference_path() -> Option<PathBuf> {
    Some(home_dir()?.join(".orbit-pi").join("updater.json"))
}

fn home_dir() -> Option<PathBuf> {
    crate::platform::home_dir_opt()
}

/// Orbit's default is to check automatically; treat an absent or unreadable
/// file as "not answered yet".
fn read_automatic_preference(path: &Path) -> bool {
    let Ok(contents) = std::fs::read_to_string(path) else {
        return true;
    };
    serde_json::from_str::<serde_json::Value>(&contents)
        .ok()
        .and_then(|value| value.get("automatic")?.as_bool())
        .unwrap_or(true)
}

fn write_automatic_preference(path: &Path, enabled: bool) {
    use std::io::Write as _;
    let Some(directory) = path.parent() else {
        return;
    };
    if std::fs::create_dir_all(directory).is_err() {
        return;
    }
    // Replace through a temporary file so a crash mid-write cannot leave the
    // preference unreadable.
    let temporary = path.with_extension("json.tmp");
    let written = std::fs::File::create(&temporary).and_then(|mut file| {
        file.write_all(format!("{{\n  \"automatic\": {enabled}\n}}\n").as_bytes())?;
        file.sync_all()
    });
    if written.is_ok() {
        let _ = std::fs::rename(&temporary, path);
    } else {
        let _ = std::fs::remove_file(&temporary);
    }
}

#[cfg(unix)]
fn take_update_error(layout: &InstallLayout) -> Option<String> {
    use std::io::Read as _;
    let path = layout.error_path();
    let metadata = std::fs::symlink_metadata(&path).ok()?;
    if !metadata.file_type().is_file() || metadata.len() > MAX_ERROR_BYTES {
        return None;
    }
    let mut contents = String::new();
    std::fs::File::open(&path)
        .ok()?
        .take(MAX_ERROR_BYTES)
        .read_to_string(&mut contents)
        .ok()?;
    let _ = std::fs::remove_file(path);
    let contents = contents.trim();
    (!contents.is_empty()).then(|| contents.to_owned())
}

// ── small utilities ─────────────────────────────────────────────────────

fn unique_temp_directory(kind: &str) -> anyhow::Result<PathBuf> {
    let root = crate::platform::scratch_dir()?;
    for _ in 0..100 {
        let candidate = root.join(format!("{kind}-{}-{}", std::process::id(), unique_nonce()));
        match std::fs::create_dir(&candidate) {
            Ok(()) => return Ok(candidate),
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(error) => return Err(error.into()),
        }
    }
    anyhow::bail!("could not reserve an update staging directory")
}

static NONCE: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

fn unique_nonce() -> u64 {
    NONCE.fetch_add(1, Ordering::Relaxed)
}

/// Reading a Sparkle appcast.
///
/// Kept off the platform seams so the ordering and parsing rules — the part
/// that decides which build a user is offered — run under `cargo test` on
/// every host, not only the one that ships an updater.
mod feed {
    #[derive(Clone, Debug, PartialEq, Eq)]
    pub(super) struct AppcastItem {
        pub(super) version: String,
        pub(super) url: String,
        pub(super) signature: String,
        pub(super) length: Option<u64>,
        /// The release notes the feed carries in `<description>`, already
        /// XML-unescaped. `None` when absent or blank.
        pub(super) notes: Option<String>,
    }

    /// Every signed item in a Sparkle appcast, newest first. The native feed
    /// writers emit one `<item>` per release, each with a
    /// `sparkle:shortVersionString` and a signed `<enclosure>`; items without
    /// a signature are ignored.
    pub(super) fn items(feed: &str) -> Vec<AppcastItem> {
        let mut items: Vec<AppcastItem> = feed
            .split("<item>")
            .skip(1)
            .filter_map(|item| {
                let item = item.split("</item>").next()?;
                let enclosure = item.split_once("<enclosure")?.1.split_once('>')?.0;
                Some(AppcastItem {
                    version: element(item, "sparkle:shortVersionString")
                        .or_else(|| attribute(enclosure, "sparkle:shortVersionString"))?,
                    url: attribute(enclosure, "url")?,
                    signature: attribute(enclosure, "sparkle:edSignature")?,
                    length: attribute(enclosure, "length").and_then(|it| it.parse().ok()),
                    notes: element(item, "description").map(|notes| unescape(&notes)),
                })
            })
            .collect();
        items.sort_by(|left, right| compare_versions(&right.version, &left.version));
        items
    }

    fn attribute(tag: &str, name: &str) -> Option<String> {
        let needle = format!("{name}=\"");
        let value = tag.split_once(&needle)?.1.split_once('"')?.0;
        (!value.is_empty()).then(|| value.to_owned())
    }

    fn element(item: &str, name: &str) -> Option<String> {
        let value = item
            .split_once(&format!("<{name}>"))?
            .1
            .split_once(&format!("</{name}>"))?
            .0
            .trim();
        (!value.is_empty()).then(|| value.to_owned())
    }

    /// Decode the XML entities `appcast.py` escapes into `<description>`, so
    /// the notes render as plain text. `&amp;` must be last or an `&amp;lt;`
    /// would turn into `<`.
    fn unescape(value: &str) -> String {
        value
            .replace("&lt;", "<")
            .replace("&gt;", ">")
            .replace("&quot;", "\"")
            .replace("&apos;", "'")
            .replace("&amp;", "&")
    }

    pub(super) fn is_newer(candidate: &str, current: &str) -> bool {
        compare_versions(candidate, current) == std::cmp::Ordering::Greater
    }

    /// Compare dotted release numbers field by field; anything after a `-` or
    /// `+` is build metadata and is not ordered.
    fn compare_versions(left: &str, right: &str) -> std::cmp::Ordering {
        fn fields(version: &str) -> impl Iterator<Item = u64> + '_ {
            version
                .split(['-', '+'])
                .next()
                .unwrap_or(version)
                .split('.')
                .map(|field| field.trim().parse::<u64>().unwrap_or(0))
        }

        let mut left = fields(left);
        let mut right = fields(right);
        loop {
            match (left.next(), right.next()) {
                (None, None) => return std::cmp::Ordering::Equal,
                (left, right) => {
                    let ordering = left.unwrap_or(0).cmp(&right.unwrap_or(0));
                    if ordering != std::cmp::Ordering::Equal {
                        return ordering;
                    }
                }
            }
        }
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        const FEED: &str = r#"<?xml version="1.0" standalone="yes"?>
<rss xmlns:sparkle="http://www.andymatuschak.org/xml-namespaces/sparkle" version="2.0">
  <channel>
    <item>
      <title>0.1.4</title>
      <sparkle:shortVersionString>0.1.4</sparkle:shortVersionString>
      <enclosure url="https://releases.orbit.dev/Orbit-0.1.4.tar.gz" length="1024" type="application/octet-stream" sparkle:edSignature="oldsig" />
    </item>
    <item>
      <title>0.2.0</title>
      <sparkle:shortVersionString>0.2.0</sparkle:shortVersionString>
      <enclosure url="https://releases.orbit.dev/Orbit-0.2.0.tar.gz" length="2048" type="application/octet-stream" sparkle:edSignature="newsig" />
    </item>
  </channel>
</rss>"#;

        #[test]
        fn the_newest_signed_item_wins_regardless_of_feed_order() {
            let item = items(FEED)
                .into_iter()
                .next()
                .expect("feed has signed items");
            assert_eq!(item.version, "0.2.0");
            assert_eq!(item.signature, "newsig");
            assert_eq!(item.length, Some(2048));
            assert!(item.url.ends_with("Orbit-0.2.0.tar.gz"));
        }

        #[test]
        fn an_unsigned_enclosure_is_never_offered() {
            let unsigned = FEED.replace(" sparkle:edSignature=\"newsig\"", "");
            let item = items(&unsigned)
                .into_iter()
                .next()
                .expect("the signed item remains");
            assert_eq!(item.version, "0.1.4");
        }

        #[test]
        fn a_feed_without_signed_items_offers_nothing() {
            assert!(items("<rss></rss>").is_empty());
            assert!(items("").is_empty());
        }

        #[test]
        fn versions_compare_field_by_field_not_lexically() {
            assert!(is_newer("0.10.0", "0.9.0"));
            assert!(is_newer("0.1.10", "0.1.9"));
            assert!(!is_newer("0.1.4", "0.1.4"));
            assert!(!is_newer("0.1.3", "0.1.4"));
            assert!(!is_newer("1.2", "1.2.0"));
            assert!(is_newer("1.2.1", "1.2"));
            assert!(!is_newer("1.2.0+build.7", "1.2.0"));
        }

        #[test]
        fn history_lists_every_signed_item_newest_first() {
            let versions: Vec<_> = items(FEED)
                .iter()
                .map(|item| item.version.clone())
                .collect();
            assert_eq!(versions, ["0.2.0", "0.1.4"]);

            // An unsigned item is never offered, so it never reaches history.
            let unsigned = FEED.replace(" sparkle:edSignature=\"newsig\"", "");
            let versions: Vec<_> = items(&unsigned)
                .iter()
                .map(|item| item.version.clone())
                .collect();
            assert_eq!(versions, ["0.1.4"]);
        }

        #[test]
        fn release_notes_come_from_the_items_description() {
            let with_notes = FEED.replace(
                "<sparkle:shortVersionString>0.2.0</sparkle:shortVersionString>",
                "<sparkle:shortVersionString>0.2.0</sparkle:shortVersionString>\n      \
                 <description>### Added\n\n- Faster &amp; safer updates</description>",
            );
            let item = items(&with_notes)
                .into_iter()
                .next()
                .expect("feed has signed items");
            assert_eq!(
                item.notes.as_deref(),
                Some("### Added\n\n- Faster & safer updates")
            );
        }

        #[test]
        fn a_feed_without_a_description_has_no_notes() {
            let item = items(FEED)
                .into_iter()
                .next()
                .expect("feed has signed items");
            assert_eq!(item.notes, None);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_absent_preference_file_leaves_automatic_checks_on() {
        let directory =
            std::env::temp_dir().join(format!("orbit-updater-preference-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&directory);
        let path = directory.join("updater.json");

        assert!(read_automatic_preference(&path));
        write_automatic_preference(&path, false);
        assert!(!read_automatic_preference(&path));
        write_automatic_preference(&path, true);
        assert!(read_automatic_preference(&path));

        let _ = std::fs::remove_dir_all(&directory);
    }

    #[test]
    fn release_versions_cannot_become_paths() {
        assert!(validate_release_version("1.2.3-beta.1").is_ok());
        assert!(validate_release_version("../../tmp/payload").is_err());
        assert!(validate_release_version("1.2.3/evil").is_err());
        assert!(validate_release_version("").is_err());
    }

    #[test]
    fn http_failure_prefers_the_root_cause() {
        #[derive(Debug)]
        struct Leaf;
        impl std::fmt::Display for Leaf {
            fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                f.write_str("invalid peer certificate: UnknownIssuer")
            }
        }
        impl std::error::Error for Leaf {}

        #[derive(Debug)]
        struct Wrapper(Leaf);
        impl std::fmt::Display for Wrapper {
            fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                write!(
                    f,
                    "error sending request for url (https://github.com/example/feed.xml)"
                )
            }
        }
        impl std::error::Error for Wrapper {
            fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
                Some(&self.0)
            }
        }

        assert_eq!(
            deepest_cause(&Wrapper(Leaf)),
            "invalid peer certificate: UnknownIssuer"
        );
    }

    /// The interop the whole update path rests on: a signature produced by
    /// the release tooling verifies here, and a tampered payload does not.
    #[test]
    fn a_release_signature_verifies_and_tampering_does_not() {
        const PUBLIC: &str = "7gZ3dbx+MPQD4vc2dk7olL9QU66JIjpJ1iqNNafU2lQ=";
        const SIGNATURE: &str = "eBIPKGvQSxFIVNwOzNjzHYs/AGiYFIe3pGulv0TeocoMN0+0l28OJZrlJ2ZuQnNBfif10VW3virGo+7GP3TwCw==";
        const PAYLOAD: &[u8] = b"Waku-0.0.0-x86_64-Setup.exe contents";

        let decode = |value: &str| {
            base64::engine::general_purpose::STANDARD
                .decode(value)
                .expect("the test vector is valid base64")
        };
        let key = ed25519_dalek::VerifyingKey::from_bytes(
            &<[u8; 32]>::try_from(decode(PUBLIC)).expect("32-byte public key"),
        )
        .expect("the test vector is a valid key");
        let signature = ed25519_dalek::Signature::from_bytes(
            &<[u8; 64]>::try_from(decode(SIGNATURE)).expect("64-byte signature"),
        );

        key.verify_strict(PAYLOAD, &signature)
            .expect("a release signature must verify");
        assert!(
            key.verify_strict(b"tampered installer", &signature)
                .is_err(),
            "a modified download must not verify"
        );
    }

    /// A real release tarball carries an explicit entry for every directory.
    /// Extraction must keep those entries directories (not empty files, which
    /// used to fail the next nested entry with `EEXIST`) and preserve the
    /// executable bit the swapped-in build relaunches with.
    #[cfg(unix)]
    #[test]
    fn extraction_keeps_directory_entries_and_executable_modes() {
        use std::os::unix::fs::PermissionsExt as _;

        let root = std::env::temp_dir().join(format!("orbit-extract-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let source = root.join("source").join("Contents");
        std::fs::create_dir_all(source.join("MacOS")).expect("create source tree");
        std::fs::write(source.join("Info.plist"), b"plist").expect("write plist");
        let binary = source.join("MacOS").join("Orbit Pi");
        std::fs::write(&binary, b"binary").expect("write binary");
        std::fs::set_permissions(&binary, std::fs::Permissions::from_mode(0o755)).expect("chmod");

        let archive = root.join("release.tar.gz");
        let file = std::fs::File::create(&archive).expect("create archive");
        let encoder = flate2::write::GzEncoder::new(file, flate2::Compression::default());
        let mut builder = tar::Builder::new(encoder);
        builder
            .append_dir_all("Orbit Pi.app", root.join("source"))
            .expect("append the release tree");
        builder
            .into_inner()
            .expect("finish the archive")
            .finish()
            .expect("finish the gzip stream");

        let destination = root.join("destination");
        std::fs::create_dir_all(&destination).expect("create destination");
        extract_release_archive(&archive, &destination, std::ffi::OsStr::new("Orbit Pi.app"))
            .expect("extract the release");

        assert!(
            destination.join("Contents").is_dir(),
            "a directory entry must extract as a directory"
        );
        let extracted = destination.join("Contents").join("MacOS").join("Orbit Pi");
        assert!(extracted.is_file(), "the executable must extract as a file");
        let mode = extracted.metadata().expect("metadata").permissions().mode() & 0o777;
        assert_eq!(mode, 0o755, "the executable bit must survive extraction");

        let _ = std::fs::remove_dir_all(&root);
    }

    /// The helper must only ever swap in a staging directory this prefix
    /// created — never an arbitrary path, another app's prefix, or the
    /// install itself.
    #[cfg(unix)]
    #[test]
    fn only_namespaced_staging_siblings_may_be_swapped_in() {
        let parent = Path::new("/home/user/.local");
        let install = parent.join("orbit-pi");
        assert!(is_staging_sibling(
            &install,
            &parent.join(".orbit-pi.update-42-0")
        ));
        assert!(!is_staging_sibling(&install, &install));
        assert!(!is_staging_sibling(
            &install,
            &parent.join(".other-app.update-42-0")
        ));
        assert!(!is_staging_sibling(
            &install,
            &parent.join("nested").join(".orbit-pi.update-42-0")
        ));
    }

    #[cfg(unix)]
    #[test]
    fn executable_identity_detects_replacement_at_same_path() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("exe");
        std::fs::write(&path, b"old").unwrap();
        let running = std::fs::File::open(&path).unwrap();
        assert!(same_executable(&running, &path));
        std::fs::rename(&path, temp.path().join("old")).unwrap();
        std::fs::write(&path, b"new").unwrap();
        assert!(!same_executable(&running, &path));
        std::fs::remove_file(&path).unwrap();
        assert!(!same_executable(&running, &path));
    }

    #[cfg(unix)]
    #[test]
    fn post_handoff_error_relaunches_existing_install() {
        use std::os::unix::fs::PermissionsExt as _;
        let temp = tempfile::tempdir().unwrap();
        let install = temp.path().join("orbit-pi");
        let executable = relaunch_executable(&install, "orbit-pi");
        std::fs::create_dir_all(executable.parent().unwrap()).unwrap();
        let marker = temp.path().join("relaunched");
        let error_marker = temp.path().join(".orbit-pi.update-error");
        std::fs::write(
            &executable,
            format!(
                "#!/bin/sh\ngrep -qx 'revalidation failed' '{}' && touch '{}'\n",
                error_marker.display(),
                marker.display()
            ),
        )
        .unwrap();
        std::fs::set_permissions(&executable, std::fs::Permissions::from_mode(0o755)).unwrap();
        let flags = HelperFlags {
            install_dir: install,
            staged_dir: temp.path().join("staged"),
            parent_pid: 0,
            ready_file: temp.path().join("ready"),
        };
        assert_eq!(
            relaunch_after_error(&flags, "orbit-pi", "revalidation failed"),
            1
        );
        for _ in 0..100 {
            if marker.exists() {
                return;
            }
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        panic!("relaunched install did not observe the error marker");
    }

    #[cfg(unix)]
    #[test]
    fn update_parent_lock_serializes_helpers() {
        use std::os::unix::io::AsRawFd as _;
        let temp = tempfile::tempdir().unwrap();
        let lock = lock_update_parent(temp.path()).unwrap();
        let other = std::fs::File::open(temp.path()).unwrap();
        assert_eq!(
            unsafe { libc::flock(other.as_raw_fd(), libc::LOCK_EX | libc::LOCK_NB) },
            -1
        );
        drop(lock);
        assert_eq!(
            unsafe { libc::flock(other.as_raw_fd(), libc::LOCK_EX | libc::LOCK_NB) },
            0
        );
    }

    #[cfg(unix)]
    #[test]
    fn linux_replacement_requires_dedicated_clean_release_tree() {
        let temp = tempfile::tempdir().unwrap();
        let install = temp
            .path()
            .canonicalize()
            .unwrap()
            .join("orbit-pi-0.2.17-x86_64-unknown-linux-gnu");
        std::fs::create_dir_all(install.join("bin")).unwrap();
        std::fs::write(install.join("bin/orbit-pi"), b"binary").unwrap();
        assert!(linux_replaceable_install(&install));
        std::fs::write(install.join("bin/other"), b"user data").unwrap();
        assert!(!linux_replaceable_install(&install));
        std::fs::remove_file(install.join("bin/other")).unwrap();
        std::os::unix::fs::symlink("/tmp", install.join("share")).unwrap();
        assert!(!linux_replaceable_install(&install));
    }

    #[cfg(all(unix, not(target_os = "macos")))]
    #[test]
    fn helper_rejects_shared_prefix_without_touching_user_data() {
        let temp = tempfile::tempdir().unwrap();
        let install = temp.path().join(".local");
        std::fs::create_dir_all(install.join("bin")).unwrap();
        std::fs::write(install.join("bin/orbit-pi"), b"binary").unwrap();
        std::fs::write(install.join("precious"), b"user data").unwrap();
        let flags = HelperFlags {
            install_dir: install.clone(),
            staged_dir: temp.path().join("..local.update-test"),
            parent_pid: unsafe { libc::getppid() as u32 },
            ready_file: temp.path().join("ready"),
        };
        assert_eq!(run_install(&flags), 1);
        assert_eq!(
            std::fs::read(install.join("precious")).unwrap(),
            b"user data"
        );
    }

    /// An archive whose single top-level directory names another build must not
    /// be extracted into the install's staging directory.
    #[cfg(unix)]
    #[test]
    fn extraction_rejects_an_unexpected_root() {
        let root = std::env::temp_dir().join(format!("orbit-extract-root-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let source = root.join("source").join("Contents");
        std::fs::create_dir_all(&source).expect("create source tree");
        std::fs::write(source.join("Info.plist"), b"plist").expect("write plist");

        let archive = root.join("release.tar.gz");
        let file = std::fs::File::create(&archive).expect("create archive");
        let encoder = flate2::write::GzEncoder::new(file, flate2::Compression::default());
        let mut builder = tar::Builder::new(encoder);
        builder
            .append_dir_all("Other.app", root.join("source"))
            .expect("append the release tree");
        builder
            .into_inner()
            .expect("finish the archive")
            .finish()
            .expect("finish the gzip stream");

        let destination = root.join("destination");
        std::fs::create_dir_all(&destination).expect("create destination");
        assert!(
            extract_release_archive(&archive, &destination, std::ffi::OsStr::new("Orbit Pi.app"),)
                .is_err(),
            "an archive rooted at another name must be rejected"
        );

        let _ = std::fs::remove_dir_all(&root);
    }

    /// Manual: build with `ORBIT_UPDATE_PUBLIC_KEY` set and run
    /// `ORBIT_FORCE_UPDATER=1 cargo test -p orbit-pi forced_init -- --ignored`.
    /// A forced build outside a managed install keeps the checker (so the
    /// modal can run) but never claims it can install.
    #[cfg(unix)]
    #[test]
    #[ignore = "manual: needs ORBIT_UPDATE_PUBLIC_KEY at build time and a real home"]
    fn forced_init_keeps_the_checker_outside_a_managed_install() {
        std::env::set_var("ORBIT_FORCE_UPDATER", "1");
        let updater = Updater::init().expect("a forced build with a key must arm the updater");
        assert!(
            !updater.can_install(),
            "a bare test binary is not a managed install"
        );
        std::env::remove_var("ORBIT_FORCE_UPDATER");
    }

    /// Manual: `cargo test -p orbit-pi live_feed -- --ignored`. Hits the real
    /// release feed and proves the check-only path parses history without a
    /// managed install. The feed must be published (signing key configured).
    #[test]
    #[ignore = "hits the real update feed"]
    fn live_feed_reports_history_without_an_install() {
        let fetched = fetch_feed().expect("the update feed must answer");
        let newest = fetched
            .history
            .first()
            .expect("the feed carries at least one signed release")
            .version
            .clone();
        if feed::is_newer(&newest, env!("CARGO_PKG_VERSION")) {
            let offer = fetched.offer.expect("a newer feed must offer a release");
            assert_eq!(offer.version, newest);
            assert!(
                offer.notes.is_some(),
                "the offered release carries its notes"
            );
        }
    }

    /// Manual: `cargo test -p orbit-pi live_artifact -- --ignored`. Downloads a
    /// real release artifact end to end and proves the streaming read, the
    /// declared length, and the progress reporting work against GitHub's
    /// release CDN. Signature verification needs a build with
    /// `ORBIT_UPDATE_PUBLIC_KEY`, so a keyless local build checks everything
    /// but that.
    #[test]
    #[ignore = "downloads a real release artifact"]
    fn live_artifact_streams_with_progress() {
        let document = http_get(FEED_URL).expect("the update feed must answer");
        let item = feed::items(&document)
            .into_iter()
            .next()
            .expect("the feed carries a signed release");
        let declared = item
            .length
            .filter(|length| *length > 0)
            .expect("the feed declares an artifact length");

        let directory = unique_temp_directory("live-artifact").expect("a temp directory");
        let archive = directory.join(artifact_name());
        let mut ticks: Vec<(u64, u64)> = Vec::new();
        let downloaded = download_to(
            &item.url,
            &archive,
            600,
            MAX_ARTIFACT_BYTES,
            declared,
            &mut |done, total| ticks.push((done, total)),
            &AtomicBool::new(false),
        );
        let cleanup = std::fs::remove_dir_all(&directory);
        downloaded.expect("the artifact must download");
        cleanup.expect("the temp directory is ours to remove");

        assert!(ticks.len() > 1, "progress is reported while bytes arrive");
        assert_eq!(ticks.first(), Some(&(0, declared)), "the start is reported");
        assert_eq!(
            ticks.last().map(|tick| tick.0),
            Some(declared),
            "the download lands on the declared length"
        );
        assert!(
            ticks.windows(2).all(|pair| pair[1].0 >= pair[0].0),
            "progress never goes backwards: {ticks:?}"
        );
    }

    /// The changelog link is the release page of the repository the feed
    /// points at, and a version that could become a path never produces one.
    #[test]
    fn the_changelog_link_is_the_release_page_of_the_feed() {
        let url = changelog_url("1.2.3").expect("a GitHub feed names its release page");
        assert!(url.starts_with("https://github.com/"), "{url}");
        assert!(url.ends_with("/releases/tag/v1.2.3"), "{url}");
        assert_eq!(changelog_url("../../etc/passwd"), None);
        assert_eq!(changelog_url(""), None);
    }

    /// Progress is throttled to about one event per percent, with the final
    /// byte always reported so the bar can land on full.
    #[test]
    fn progress_events_throttle_to_one_per_percent() {
        let total = 40 * 1024 * 1024;
        let mut throttle = ProgressThrottle::default();
        let mut emitted = 0;
        for byte in (0..=total).step_by(64 * 1024) {
            if throttle.should_emit(byte, total) {
                emitted += 1;
            }
        }
        assert!(
            (1..=101).contains(&emitted),
            "one event per percent, not one per chunk: {emitted}"
        );
        let mut throttle = ProgressThrottle::default();
        assert!(
            throttle.should_emit(0, total),
            "the start is always reported"
        );
        assert!(
            throttle.should_emit(total, total),
            "the final byte is always reported"
        );
    }
}
