use super::*;
use crate::pi_update;
use gpui::Timer;

/// How long the launch check waits before touching the network, so the
/// session's own startup traffic (state, catalogs, quota) goes first.
const PI_CHECK_DELAY: Duration = Duration::from_secs(3);

/// How long the installer waits for a quiet moment before giving up. Swapping
/// pi's files under a live run can break its lazy imports, so a busy session
/// defers the update to the next launch rather than risk the turn.
const PI_QUIET_WAIT: Duration = Duration::from_secs(120);
const PI_QUIET_POLL: Duration = Duration::from_secs(5);

impl OrbitApp {
    /// Check for a newer pi in the background at launch and install it with
    /// pi's own updater (`pi update self`). Quiet when pi is current, the
    /// release endpoint is unreachable, or `PI_BIN` points at a custom build
    /// — a user-managed binary is never touched. pi's own offline switch
    /// (`PI_OFFLINE`) skips the check the same way it skips pi's.
    pub(super) fn check_pi_update_on_launch(&mut self, cx: &mut Context<Self>) {
        if std::env::var_os(orbit_rpc::PI_BIN_ENV).is_some_and(|bin| !bin.is_empty()) {
            return;
        }
        if std::env::var_os("PI_OFFLINE").is_some_and(|value| !value.is_empty()) {
            return;
        }
        // The launch dependency probe already read `pi --version`; no update
        // without an installed pi to update.
        let Some(installed) = self
            .deps
            .iter()
            .find(|dep| dep.bin == "pi")
            .and_then(|dep| dep.version.clone())
        else {
            return;
        };
        cx.spawn(async move |this, cx| {
            Timer::after(PI_CHECK_DELAY).await;
            let latest = cx
                .background_executor()
                .spawn(async { pi_update::latest_version() })
                .await;
            let Some(latest) = latest else { return };
            if !pi_update::is_newer(&latest, &installed) {
                return;
            }
            if this
                .update(cx, |app, cx| {
                    app.toast_info(tr!("pi_update.available_background", version = latest));
                    cx.notify();
                })
                .is_err()
            {
                return;
            }
            // Install only between runs: a live session may still lazily
            // import files the package manager is about to replace.
            let mut quiet = false;
            let deadline = Instant::now() + PI_QUIET_WAIT;
            loop {
                let busy = match this.update(cx, |app, _| app.pi_busy()) {
                    Ok(busy) => busy,
                    // The app is gone; nothing left to update for.
                    Err(_) => return,
                };
                if !busy {
                    quiet = true;
                    break;
                }
                if Instant::now() >= deadline {
                    break;
                }
                Timer::after(PI_QUIET_POLL).await;
            }
            if !quiet {
                let _ = this.update(cx, |app, cx| {
                    app.toast_info(tr!("pi_update.available_next_launch", version = latest));
                    cx.notify();
                });
                return;
            }
            let (result, updated, deps) = cx
                .background_executor()
                .spawn(async {
                    let result = pi_update::run_self_update();
                    let updated = pi_update::installed_version();
                    (result, updated, onboarding::check_dependencies())
                })
                .await;
            let _ = this.update(cx, |app, cx| {
                app.finish_pi_update(&installed, result, updated, deps, cx);
            });
        })
        .detach();
    }

    /// True while any pi session (the active one or a parked one) is running,
    /// streaming, or compacting — i.e. when pi's files must not be replaced.
    fn pi_busy(&self) -> bool {
        self.busy
            || self.transcript.is_streaming()
            || self.is_compacting
            || self.lives.values().any(|parked| parked.busy)
    }

    /// Apply the result of `pi update self`. Live pi processes keep the old
    /// version; only sessions started after this see the update.
    pub(super) fn finish_pi_update(
        &mut self,
        installed: &str,
        result: std::io::Result<std::process::Output>,
        updated: Option<String>,
        deps: Vec<onboarding::Dependency>,
        cx: &mut Context<Self>,
    ) {
        match result {
            Ok(output) if output.status.success() => {
                if let Some(version) = updated.filter(|v| pi_update::is_newer(v, installed)) {
                    // Keep the dependency board truthful about what is on disk.
                    self.deps = deps;
                    self.toast_success(tr!("pi_update.updated", version = version));
                }
                // Otherwise pi reported no change (a stale release endpoint or
                // a managed install pi left alone): nothing to announce.
            }
            Ok(output) => {
                let detail = pi_update::failure_detail(&output.stdout, &output.stderr);
                self.toast_error(tr!("pi_update.failed", detail = detail));
            }
            Err(err) => self.toast_error(tr!("pi_update.failed_detail", error = err)),
        }
        cx.notify();
    }
}
