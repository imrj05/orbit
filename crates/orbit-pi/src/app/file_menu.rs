//! The transcript text menu's file items (issue #199): Open, Reveal in File
//! Manager, Copy Path and Save a Copy As… on a path the agent wrote. The
//! transcript only spots the path; this resolves it against the session's
//! folder and does the file work off the UI thread, reporting through toasts.
use super::*;
use crate::transcript_view::{FileAction, FileOpener};

impl OrbitApp {
    /// Handed to the transcript each frame. A remote session's files are on
    /// the server, not this machine, so it gets no file items at all.
    pub(super) fn file_opener(&self, cx: &Context<Self>) -> Option<FileOpener> {
        if self.remote.is_some() {
            return None;
        }
        let this = cx.weak_entity();
        Some(Rc::new(move |written, action, _, cx| {
            this.update(cx, |app, cx| app.run_file_action(written, action, cx))
                .ok();
        }))
    }

    fn run_file_action(&mut self, written: String, action: FileAction, cx: &mut Context<Self>) {
        let path = crate::path_ref::resolve(
            &written,
            &self.workspace_dir(),
            &crate::platform::home_dir(),
        );
        if action == FileAction::CopyPath {
            cx.write_to_clipboard(ClipboardItem::new_string(path.display().to_string()));
            self.toast_success(tr!("transcript.path_copied"));
            cx.notify();
            return;
        }
        cx.spawn(async move |this, cx| {
            let probe = path.clone();
            let kind = cx
                .background_executor()
                .spawn(async move { std::fs::metadata(&probe).map(|meta| meta.is_dir()).ok() })
                .await;
            let Some(is_dir) = kind else {
                let _ = this.update(cx, |app, cx| {
                    app.toast_warning(tr!(
                        "transcript.file_missing",
                        path = path.display().to_string()
                    ));
                    cx.notify();
                });
                return;
            };
            match action {
                FileAction::Open => {
                    let _ = cx.update(|cx| cx.open_with_system(&path));
                }
                FileAction::Reveal => {
                    let target = path.clone();
                    cx.background_executor()
                        .spawn(async move { crate::platform::reveal_in_file_manager(&target) })
                        .await;
                }
                FileAction::SaveAs if is_dir => {
                    let _ = this.update(cx, |app, cx| {
                        app.toast_info(tr!("transcript.save_folder_unsupported"));
                        cx.notify();
                    });
                }
                FileAction::SaveAs => {
                    let name = path
                        .file_name()
                        .map(|name| name.to_string_lossy().into_owned())
                        .unwrap_or_default();
                    let Some(handle) = rfd::AsyncFileDialog::new()
                        .set_file_name(name)
                        .save_file()
                        .await
                    else {
                        return;
                    };
                    let target = handle.path().to_path_buf();
                    let source = path.clone();
                    let copied = cx
                        .background_executor()
                        .spawn(async move { std::fs::copy(&source, &target).map(|_| target) })
                        .await;
                    let _ = this.update(cx, |app, cx| {
                        match copied {
                            Ok(target) => app.toast_success(tr!(
                                "transcript.file_saved",
                                path = target.display().to_string()
                            )),
                            Err(err) => app.toast_error(tr!(
                                "transcript.file_save_failed",
                                error = err.to_string()
                            )),
                        }
                        cx.notify();
                    });
                }
                FileAction::CopyPath => {}
            }
        })
        .detach();
    }
}
