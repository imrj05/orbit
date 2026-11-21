//! Remote (SSH) sessions in the app: where a new task runs, the one spawn
//! seam every restart goes through, and the honest "not on this machine"
//! state for surfaces that read the local disk. Hosts are managed in
//! Settings → Remote hosts (`remote_hosts_ui.rs`).
//!
//! Only the agent runs on the host (see `crates/orbit-pi/docs/remote-sessions.md`).
//! The explorer, Review, Git, Files and Worktrees describe the local disk, so
//! while [`OrbitApp::remote`] is set they render [`remote_unavailable`]
//! instead of the wrong machine's files.

use super::*;
use crate::remote_hosts::RemoteHost;
use crate::ui::prelude::*;

/// Where a new task runs.
pub(crate) enum TaskPlace {
    /// A local folder; it becomes the current workspace.
    Folder(PathBuf),
    /// A configured remote host; the local workspace is left as it was.
    Remote(RemoteHost),
}

impl OrbitApp {
    /// Spawn pi for the session this view is showing: on the remote host
    /// while the session is remote, otherwise locally in `workspace`. Every
    /// restart path (Start, reconnect, MCP apply, forced-stop respawn) goes
    /// through here so a remote session never quietly comes back local.
    pub(super) fn spawn_pi(
        &mut self,
        workspace: &Path,
        apply_default: bool,
    ) -> anyhow::Result<orbit_rpc::PiClient> {
        if self.account_switching {
            anyhow::bail!("Wait for the account switch to finish");
        }
        let decision = self.project_trust_for(workspace);
        self.prompt_for_unknown_trust(workspace, decision);
        let approve_project = decision.is_trusted();
        match &self.remote {
            Some(host) => {
                let secret = self.remote_secret(host);
                self.extensions.spawn_remote(
                    &host.target()?,
                    secret.as_deref(),
                    self.remote_launch_workflow().map(WorkflowMode::as_wire),
                    approve_project,
                )
            }
            None => {
                if apply_default && self.current_session_path.is_none() {
                    self.provider_accounts.begin_session()?;
                }
                let routes = self.provider_accounts.routes()?;
                let mcp_env = if approve_project {
                    self.mcp.secret_env()
                } else {
                    Vec::new()
                };
                self.extensions.spawn_with_accounts(
                    workspace,
                    apply_default,
                    &mcp_env,
                    &routes,
                    approve_project,
                )
            }
        }
    }

    /// The workflow mode a remote pi starts in. The host's store has no entry
    /// for a session it has not started yet, so the launch carries the mode
    /// as `ORBIT_WORKFLOW_MODE`: a New Task choice, an exact stored mode when
    /// one exists, otherwise Build for a new session. Resumed or ambiguous
    /// sessions get no launch value, so their own store or branch record
    /// decides instead of an invented Build masking Plan/Ask.
    pub(super) fn remote_launch_workflow(&self) -> Option<WorkflowMode> {
        if let Some(mode) = self.workflow_pending {
            return Some(mode);
        }
        // A resumed session's own store entry or branch record decides. An
        // invalid/missing local value must never be normalized to Build and
        // then used to mask a valid Plan/Ask branch entry.
        if self.remote_resuming {
            return None;
        }
        match self.session_id.as_deref() {
            Some(session_id) => crate::workflow::load_valid_for(session_id),
            None => Some(WorkflowMode::default()),
        }
    }

    /// Mirror a workflow change into the host's `~/.orbit-pi/workflow.json`,
    /// which the workflow extension there reads on every hook. Runs over a
    /// short ssh of its own, off the UI thread; a failure says the mode did
    /// not reach the host rather than letting the chip claim it did.
    pub(super) fn sync_remote_workflow(
        &mut self,
        session_id: String,
        mode: WorkflowMode,
        cx: &mut Context<Self>,
    ) {
        let Some(host) = self.remote.clone() else {
            return;
        };
        let secret = self.remote_secret(&host);
        cx.spawn(async move |this, cx| {
            let host_name = host.name.clone();
            let host_for_text = host.clone();
            let outcome = cx
                .background_executor()
                .spawn(async move {
                    crate::remote_hosts::set_workflow_mode(
                        &host,
                        secret.as_deref(),
                        &session_id,
                        mode.as_wire(),
                    )
                })
                .await;
            if let Err(failure) = outcome {
                let _ = this.update(cx, |app, cx| {
                    app.toast_warning(tr!(
                        "remote.workflow_sync_failed",
                        mode = mode.label(),
                        host = host_name,
                        error = super::remote_providers_ui::failure_text(failure, &host_for_text)
                    ));
                    cx.notify();
                });
            }
        })
        .detach();
    }

    /// Refuse `count` file attachments while the session is remote, with the
    /// reason. Returns whether the caller should stop. Images are unaffected:
    /// they travel inside the prompt itself.
    pub(super) fn refuse_files_for_remote(&mut self, count: usize) -> bool {
        if count == 0 {
            return false;
        }
        let Some(host) = self.remote_host_name().map(str::to_owned) else {
            return false;
        };
        self.toast_warning(tr!("remote.files_unavailable", host = host));
        true
    }

    /// The remote host's name while the active session runs there.
    pub(super) fn remote_host_name(&self) -> Option<&str> {
        self.remote.as_ref().map(|host| host.name.as_str())
    }

    /// Say why a local-disk surface is unavailable, instead of opening it on
    /// the wrong machine. Returns whether the caller should stop.
    pub(super) fn block_local_surface_for_remote(&mut self) -> bool {
        let Some(name) = self.remote_host_name().map(str::to_owned) else {
            return false;
        };
        self.toast_info(tr!("remote.surface_unavailable", host = name));
        true
    }
}

/// The page body for Files / Worktrees while the session is remote (Git has
/// its own read-only page, `remote_git_ui.rs`).
pub(super) fn remote_unavailable(theme: Theme, host: &str) -> AnyElement {
    empty_state(
        theme,
        "icons/server-stack.svg",
        &tr!("remote.unavailable_title", host = host.to_string()),
        Some(&tr!("remote.unavailable_detail")),
        EmptyFill::Full,
    )
}
