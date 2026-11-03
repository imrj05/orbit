//! Export the visible transcript without writing session files or uploading data.

use super::*;
use std::io::Write as _;
use std::path::Path;

/// Create a new file only; never replace an existing file silently.
fn write_new_markdown(path: &Path, body: &str) -> std::io::Result<()> {
    let mut file = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)?;
    file.write_all(body.as_bytes())
}

impl OrbitApp {
    /// Hand the transcript a callback that saves the clicked response.
    pub(super) fn response_exporter(
        &self,
        cx: &Context<Self>,
    ) -> crate::transcript_view::ResponseExporter {
        let this = cx.weak_entity();
        Rc::new(move |body, _, cx| {
            this.update(cx, |app, cx| app.export_response_markdown(body, cx))
                .ok();
        })
    }

    /// Snapshot the visible transcript before opening the asynchronous save
    /// panel. A later session switch must not change what gets written.
    pub(super) fn export_markdown(&mut self, last_response: bool, cx: &mut Context<Self>) {
        let body = if last_response {
            self.transcript.export_last_response_markdown()
        } else {
            self.transcript.export_visible_markdown()
        };
        let Some(body) = body else {
            self.toast_info(tr!("transcript_export.empty"));
            cx.notify();
            return;
        };
        self.save_markdown(body, last_response, cx);
    }

    /// Save the text captured from the exact response whose footer was clicked.
    pub(super) fn export_response_markdown(&mut self, body: String, cx: &mut Context<Self>) {
        if body.is_empty() {
            self.toast_info(tr!("transcript_export.empty"));
            cx.notify();
            return;
        }
        self.save_markdown(body, true, cx);
    }

    fn save_markdown(&mut self, body: String, last_response: bool, cx: &mut Context<Self>) {
        cx.spawn(async move |this, cx| {
            let Some(handle) = rfd::AsyncFileDialog::new()
                .set_file_name(if last_response {
                    "orbit-response.md"
                } else {
                    "orbit-transcript.md"
                })
                .add_filter("Markdown", &["md"])
                .save_file()
                .await
            else {
                return;
            };
            let path = handle.path().to_path_buf();
            let target = path.clone();
            let result = cx
                .background_executor()
                .spawn(async move {
                    // Refuse overwrites even if a platform's save panel did
                    // not ask; the user can choose a new name and retry.
                    write_new_markdown(&target, &body)
                })
                .await;
            let _ = this.update(cx, |app, cx| {
                match result {
                    Ok(()) => app.toast_success(tr!(
                        "transcript_export.saved",
                        path = path.display().to_string()
                    )),
                    Err(error) => {
                        app.toast_error(tr!("transcript_export.failed", error = error.to_string()))
                    }
                }
                cx.notify();
            });
        })
        .detach();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn markdown_export_never_overwrites_an_existing_file() {
        let dir = std::env::temp_dir().join(format!("orbit-export-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("existing.md");
        std::fs::write(&path, "keep me").unwrap();
        let error = write_new_markdown(&path, "replacement").unwrap_err();
        assert_eq!(error.kind(), std::io::ErrorKind::AlreadyExists);
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "keep me");
        std::fs::remove_dir_all(dir).unwrap();
    }
}
