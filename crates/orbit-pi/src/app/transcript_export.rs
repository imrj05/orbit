//! Export the visible transcript without writing session files or uploading data.

use super::*;
use std::io::Write as _;
use std::path::Path;

fn pdf_fonts() -> markdown2pdf::fonts::FontConfig {
    use markdown2pdf::fonts::{FontConfig, FontSource};
    FontConfig::new()
        .with_default_font_source(FontSource::bytes(include_bytes!(
            "../../assets/fonts/bundled/noto-sans/NotoSans-Regular.ttf"
        )))
        .with_code_font_source(FontSource::bytes(include_bytes!(
            "../../assets/fonts/bundled/noto-sans-mono/NotoSansMono-Regular.ttf"
        )))
        .with_fallback_fonts(["Arial Unicode MS", "Noto Sans CJK SC", "Noto Sans CJK JP"])
}

/// Render a snapshot, confining Markdown image references to private attachment files.
fn render_pdf(sections: Vec<crate::transcript::PdfSection>) -> anyhow::Result<Vec<u8>> {
    render_pdf_with_fonts(sections, &pdf_fonts())
}

fn render_pdf_with_fonts(
    sections: Vec<crate::transcript::PdfSection>,
    fonts: &markdown2pdf::fonts::FontConfig,
) -> anyhow::Result<Vec<u8>> {
    use markdown2pdf::config;

    let image_root = private_tempdir()?;
    let mut tokens = Vec::new();
    let mut files = Vec::new();
    for section in sections {
        let mut lexer = markdown2pdf::markdown::Lexer::new(section.markdown);
        tokens.extend(lexer.parse()?);
        tokens.extend([
            markdown2pdf::markdown::Token::Newline,
            markdown2pdf::markdown::Token::Newline,
        ]);
        for attachment in section.images {
            let mut reader = image::ImageReader::new(std::io::Cursor::new(&attachment.bytes))
                .with_guessed_format()?;
            let mut limits = image::Limits::default();
            limits.max_image_width = Some(4000);
            limits.max_image_height = Some(4000);
            limits.max_alloc = Some(128 * 1024 * 1024);
            reader.limits(limits);
            let decoded = reader
                .decode()
                .map_err(|error| anyhow::anyhow!("Could not decode an attached image: {error}"))?;
            let mut file = tempfile::NamedTempFile::new_in(image_root.path())?;
            decoded.write_to(&mut file, image::ImageFormat::Png)?;
            let name = file
                .path()
                .file_name()
                .ok_or_else(|| anyhow::anyhow!("Invalid temporary image filename"))?
                .to_string_lossy()
                .into_owned();
            tokens.push(markdown2pdf::markdown::Token::Image {
                alt: vec![markdown2pdf::markdown::Token::Text("Attached image".into())],
                url: name,
                title: None,
            });
            tokens.extend([
                markdown2pdf::markdown::Token::Newline,
                markdown2pdf::markdown::Token::Newline,
            ]);
            files.push(file);
        }
    }
    let mut style = config::load_config_strict(config::ConfigSource::Theme("github"), None)?;
    style.security.image_root = Some(image_root.path().to_owned());
    style.security.allow_absolute_image_paths = false;
    style.security.allow_remote_images = false;
    // Parse sections directly, so response text is never consumed as frontmatter.
    let result = markdown2pdf::render::render_to_bytes(tokens, style, Some(fonts))?;
    drop(files);
    Ok(result)
}

/// Suggest a portable name from the response snapshot, not the current session.
fn response_pdf_filename(body: &str) -> String {
    use unicode_segmentation::UnicodeSegmentation as _;

    const MAX_STEM_BYTES: usize = 120;
    let mut in_code = false;
    for line in body.lines() {
        let line = line.trim();
        if line.starts_with("```") || line.starts_with("~~~") {
            in_code = !in_code;
            continue;
        }
        if in_code {
            continue;
        }
        let line =
            line.trim_matches(|ch: char| ch.is_whitespace() || matches!(ch, '#' | '*' | '_' | '`'));
        let mut stem = String::new();
        for grapheme in line.graphemes(true) {
            let invalid = grapheme.chars().any(|ch| {
                ch.is_control()
                    || matches!(ch, '<' | '>' | ':' | '"' | '/' | '\\' | '|' | '?' | '*')
                    || matches!(ch, '\u{202a}'..='\u{202e}' | '\u{2066}'..='\u{2069}')
            });
            let part = if invalid { "-" } else { grapheme };
            if stem.len() + part.len() > MAX_STEM_BYTES {
                break;
            }
            stem.push_str(part);
        }
        let stem = stem.trim_matches(|ch: char| ch.is_whitespace() || matches!(ch, '.' | '-'));
        if stem.is_empty() {
            continue;
        }
        // Windows reserves these names even when followed by an extension.
        let first = stem
            .split('.')
            .next()
            .unwrap_or_default()
            .trim_end()
            .to_ascii_uppercase();
        let device_number = first
            .strip_prefix("COM")
            .or_else(|| first.strip_prefix("LPT"));
        let reserved = matches!(first.as_str(), "CON" | "PRN" | "AUX" | "NUL")
            || device_number.is_some_and(|number| {
                matches!(
                    number,
                    "1" | "2" | "3" | "4" | "5" | "6" | "7" | "8" | "9" | "¹" | "²" | "³"
                )
            });
        let name = if reserved {
            format!("response-{stem}")
        } else {
            stem.to_string()
        };
        return if name.to_ascii_lowercase().ends_with(".pdf") {
            name
        } else {
            format!("{name}.pdf")
        };
    }
    "orbit-response.pdf".to_string()
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum PreviewChoice {
    Preview,
    Skip,
    Cancel,
}

trait PdfDialogs {
    async fn disclosure(&mut self) -> bool;
    async fn preview_choice(&mut self) -> PreviewChoice;
    fn open_preview(&mut self, bytes: &[u8]) -> anyhow::Result<()>;
    async fn preview_done(&mut self) -> bool;
    async fn destination(&mut self, response: bool) -> Option<std::path::PathBuf>;
}

/// The save panel is never opened after cancellation or a preview error.
async fn choose_pdf_destination(
    dialogs: &mut impl PdfDialogs,
    bytes: &[u8],
    response: bool,
) -> anyhow::Result<Option<std::path::PathBuf>> {
    match dialogs.preview_choice().await {
        PreviewChoice::Cancel => return Ok(None),
        PreviewChoice::Preview => {
            dialogs.open_preview(bytes)?;
            if !dialogs.preview_done().await {
                return Ok(None);
            }
        }
        PreviewChoice::Skip => {}
    }
    Ok(dialogs.destination(response).await)
}

async fn prepare_pdf_export<F, Fut>(
    dialogs: &mut impl PdfDialogs,
    response: bool,
    render: F,
) -> anyhow::Result<Option<(std::path::PathBuf, Vec<u8>)>>
where
    F: FnOnce() -> Fut,
    Fut: std::future::Future<Output = anyhow::Result<Vec<u8>>>,
{
    if !dialogs.disclosure().await {
        return Ok(None);
    }
    let bytes = render().await?;
    Ok(choose_pdf_destination(dialogs, &bytes, response)
        .await?
        .map(|path| (path, bytes)))
}

struct NativePdfDialogs {
    title: String,
    disclosure: String,
    question: String,
    preview_done: String,
    file_name: String,
    previews: Vec<tempfile::TempDir>,
}

impl PdfDialogs for NativePdfDialogs {
    async fn disclosure(&mut self) -> bool {
        rfd::AsyncMessageDialog::new()
            .set_title(&self.title)
            .set_description(&self.disclosure)
            .set_buttons(rfd::MessageButtons::OkCancel)
            .show()
            .await
            == rfd::MessageDialogResult::Ok
    }

    async fn preview_choice(&mut self) -> PreviewChoice {
        match rfd::AsyncMessageDialog::new()
            .set_title(&self.title)
            .set_description(&self.question)
            .set_buttons(rfd::MessageButtons::YesNoCancel)
            .show()
            .await
        {
            rfd::MessageDialogResult::Yes => PreviewChoice::Preview,
            rfd::MessageDialogResult::No => PreviewChoice::Skip,
            _ => PreviewChoice::Cancel,
        }
    }

    fn open_preview(&mut self, bytes: &[u8]) -> anyhow::Result<()> {
        let (dir, path) = create_pdf_preview(bytes)?;
        open_pdf_preview(&path)?;
        self.previews.push(dir);
        Ok(())
    }

    async fn preview_done(&mut self) -> bool {
        rfd::AsyncMessageDialog::new()
            .set_title(&self.title)
            .set_description(&self.preview_done)
            .set_buttons(rfd::MessageButtons::OkCancel)
            .show()
            .await
            == rfd::MessageDialogResult::Ok
    }

    async fn destination(&mut self, _response: bool) -> Option<std::path::PathBuf> {
        rfd::AsyncFileDialog::new()
            .set_file_name(&self.file_name)
            .add_filter("PDF", &["pdf"])
            .save_file()
            .await
            .map(|handle| handle.path().to_path_buf())
    }
}

#[cfg(unix)]
fn private_tempdir() -> std::io::Result<tempfile::TempDir> {
    use std::os::unix::fs::PermissionsExt as _;
    tempfile::Builder::new()
        .permissions(std::fs::Permissions::from_mode(0o700))
        .tempdir()
}

#[cfg(not(unix))]
fn private_tempdir() -> std::io::Result<tempfile::TempDir> {
    tempfile::tempdir()
}

fn create_pdf_preview(bytes: &[u8]) -> std::io::Result<(tempfile::TempDir, std::path::PathBuf)> {
    let dir = private_tempdir()?;
    let path = dir.path().join("preview.pdf");
    write_new_pdf(&path, bytes)?;
    Ok((dir, path))
}

#[cfg(not(target_os = "windows"))]
fn open_pdf_preview(path: &Path) -> std::io::Result<()> {
    #[cfg(target_os = "macos")]
    let opener = "/usr/bin/open";
    #[cfg(not(target_os = "macos"))]
    let opener = "xdg-open";
    std::process::Command::new(opener).arg(path).spawn()?;
    Ok(())
}

#[cfg(target_os = "windows")]
fn open_pdf_preview(path: &Path) -> std::io::Result<()> {
    use std::os::windows::ffi::OsStrExt;
    use windows_sys::Win32::UI::Shell::ShellExecuteW;
    let path: Vec<u16> = path.as_os_str().encode_wide().chain(Some(0)).collect();
    // ShellExecute accepts the path directly, without passing it through cmd.exe.
    let result = unsafe {
        ShellExecuteW(
            std::ptr::null_mut(),
            std::ptr::null(),
            path.as_ptr(),
            std::ptr::null(),
            std::ptr::null(),
            1,
        )
    };
    if result as isize <= 32 {
        return Err(std::io::Error::other(format!(
            "Could not open PDF viewer (error {}).",
            result as isize
        )));
    }
    Ok(())
}

/// Create a new file only; never replace an existing file silently.
fn write_new_pdf(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    let mut file = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)?;
    file.write_all(bytes)
}

/// Create a new file only; never replace an existing file silently.
fn write_new_markdown(path: &Path, body: &str) -> std::io::Result<()> {
    let mut file = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)?;
    file.write_all(body.as_bytes())
}

impl OrbitApp {
    /// What the current export preference adds beyond the visible answer text.
    fn export_options(&self) -> crate::transcript::ExportOptions {
        crate::transcript::ExportOptions {
            include_thinking_and_tools: self.export_prefs.include_thinking_and_tools,
        }
    }

    /// The sentence disclosing an omission, or nothing when the user opted in.
    fn omitted_work_sentence(&self) -> String {
        if self.export_prefs.include_thinking_and_tools {
            String::new()
        } else {
            tr!("transcript_export.work_omitted").to_string()
        }
    }

    /// Hand the transcript a callback that saves the clicked response.
    pub(super) fn response_exporter(
        &self,
        cx: &Context<Self>,
    ) -> crate::transcript_view::ResponseExporter {
        let this = cx.weak_entity();
        Rc::new(move |body, pdf, row, _, cx| {
            this.update(cx, |app, cx| {
                let options = app.export_options();
                if pdf {
                    if let Some(section) = app.transcript.export_response_pdf(row, &body, options) {
                        app.export_pdf(vec![section], true, cx);
                    } else {
                        app.toast_info(tr!("transcript_export.empty"));
                        cx.notify();
                    }
                } else {
                    app.export_response_markdown(row, body, cx);
                }
            })
            .ok();
        })
    }

    /// Snapshot the visible transcript before opening the asynchronous save
    /// panel. A later session switch must not change what gets written.
    pub(super) fn export_markdown(&mut self, last_response: bool, cx: &mut Context<Self>) {
        let options = self.export_options();
        let body = if last_response {
            self.transcript.export_last_response_markdown(options)
        } else {
            self.transcript.export_visible_markdown(options)
        };
        let Some(body) = body else {
            self.toast_info(tr!("transcript_export.empty"));
            cx.notify();
            return;
        };
        self.save_markdown(body, last_response, cx);
    }

    /// Save the text captured from the exact response whose footer was clicked.
    /// The row is the identity check: a response that moved or changed since the
    /// click is not silently replaced with a different one, and the clicked body
    /// is still what gets written when the snapshot no longer matches.
    pub(super) fn export_response_markdown(
        &mut self,
        row: usize,
        body: String,
        cx: &mut Context<Self>,
    ) {
        if body.is_empty() {
            self.toast_info(tr!("transcript_export.empty"));
            cx.notify();
            return;
        }
        let options = self.export_options();
        let body = self
            .transcript
            .export_response_markdown(row, &body, options)
            .unwrap_or(body);
        self.save_markdown(body, true, cx);
    }

    pub(super) fn export_visible_pdf(&mut self, cx: &mut Context<Self>) {
        let options = self.export_options();
        let Some(sections) = self.transcript.export_visible_pdf(options) else {
            self.toast_info(tr!("transcript_export.empty"));
            cx.notify();
            return;
        };
        self.export_pdf(sections, false, cx);
    }

    pub(super) fn export_last_response_pdf(&mut self, cx: &mut Context<Self>) {
        let options = self.export_options();
        let Some(section) = self.transcript.export_last_response_pdf(options) else {
            self.toast_info(tr!("transcript_export.empty"));
            cx.notify();
            return;
        };
        self.export_pdf(vec![section], true, cx);
    }

    fn export_pdf(
        &mut self,
        sections: Vec<crate::transcript::PdfSection>,
        response: bool,
        cx: &mut Context<Self>,
    ) {
        if sections.is_empty()
            || sections
                .iter()
                .all(|section| section.markdown.is_empty() && section.images.is_empty())
        {
            self.toast_info(tr!("transcript_export.empty"));
            cx.notify();
            return;
        }
        let title = tr!("transcript_export.pdf_title").to_string();
        let description = if response {
            tr!("transcript_export.pdf_response_disclosure").to_string()
        } else {
            format!(
                "{} {}",
                tr!("transcript_export.pdf_transcript_disclosure"),
                self.omitted_work_sentence()
            )
            .trim_end()
            .to_string()
        };
        let question = tr!("transcript_export.pdf_preview_question").to_string();
        let preview_done = tr!("transcript_export.pdf_preview_done").to_string();
        let file_name = if response {
            response_pdf_filename(&sections[0].markdown)
        } else {
            "orbit-transcript.pdf".to_string()
        };
        cx.spawn(async move |this, cx| {
            let mut dialogs = NativePdfDialogs {
                title,
                disclosure: description,
                question,
                preview_done,
                file_name,
                previews: Vec::new(),
            };
            let executor = cx.background_executor().clone();
            let selected = prepare_pdf_export(&mut dialogs, response, || async move {
                executor.spawn(async move { render_pdf(sections) }).await
            })
            .await;
            // The external viewer may still have the file open after the dialog closes.
            let _ = this.update(cx, |app, _| app.pdf_previews.extend(dialogs.previews));
            let (path, bytes) = match selected {
                Ok(Some(selected)) => selected,
                Ok(None) => return,
                Err(error) => {
                    let _ = this.update(cx, |app, cx| {
                        app.toast_error(tr!(
                            "transcript_export.pdf_failed",
                            error = error.to_string()
                        ));
                        cx.notify();
                    });
                    return;
                }
            };
            let target = path.clone();
            let result = cx
                .background_executor()
                .spawn(async move { write_new_pdf(&target, &bytes) })
                .await;
            let _ = this.update(cx, |app, cx| {
                match result {
                    Ok(()) => app.toast_success(tr!(
                        "transcript_export.pdf_saved",
                        path = path.display().to_string()
                    )),
                    Err(error) => app.toast_error(tr!(
                        "transcript_export.pdf_failed",
                        error = error.to_string()
                    )),
                }
                cx.notify();
            });
        })
        .detach();
    }

    fn save_markdown(&mut self, body: String, last_response: bool, cx: &mut Context<Self>) {
        let disclosure = (!last_response).then(|| {
            (
                tr!("transcript_export.disclosure_title").to_string(),
                format!(
                    "{} {}",
                    tr!("transcript_export.disclosure"),
                    self.omitted_work_sentence()
                )
                .trim_end()
                .to_string(),
            )
        });
        let saved_note = format!(
            "{} {}",
            tr!("transcript_export.saved_note"),
            self.omitted_work_sentence()
        )
        .trim_end()
        .to_string();
        cx.spawn(async move |this, cx| {
            if let Some((title, description)) = disclosure {
                let result = rfd::AsyncMessageDialog::new()
                    .set_title(title)
                    .set_description(description)
                    .set_buttons(rfd::MessageButtons::OkCancel)
                    .show()
                    .await;
                if result != rfd::MessageDialogResult::Ok {
                    return;
                }
            }
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
                        path = path.display().to_string(),
                        omitted = saved_note
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
    fn response_pdf_filename_uses_the_responses_first_meaningful_line() {
        assert_eq!(
            response_pdf_filename("\n# **Deployment summary**\n\nDetails"),
            "Deployment summary.pdf"
        );
        assert_eq!(
            response_pdf_filename("```rust\nfn main() {}\n```\n\n## Results"),
            "Results.pdf"
        );
        assert_eq!(
            response_pdf_filename("\nplain answer\nmore"),
            "plain answer.pdf"
        );
        assert_eq!(
            response_pdf_filename("```\nonly code\n```"),
            "orbit-response.pdf"
        );
        assert_eq!(response_pdf_filename("###\n  \n"), "orbit-response.pdf");
    }

    #[test]
    fn response_pdf_filename_is_portable_and_keeps_unicode() {
        assert_eq!(
            response_pdf_filename("../../Plan: rollout?"),
            "Plan- rollout.pdf"
        );
        assert_eq!(response_pdf_filename("CON.txt"), "response-CON.txt.pdf");
        assert_eq!(response_pdf_filename("Report.pdf"), "Report.pdf");
        assert_eq!(response_pdf_filename("lpt¹"), "response-lpt¹.pdf");
        assert_eq!(response_pdf_filename("नमस्ते 🪐"), "नमस्ते 🪐.pdf");
        let long = response_pdf_filename(&"🪐".repeat(100));
        assert!(long.len() <= 124);
        assert!(long.ends_with(".pdf"));
        assert!(!long.contains(['/', '\\', ':', '*', '?', '"', '<', '>', '|']));
        assert_eq!(response_pdf_filename("Report\u{202e}fdp"), "Report-fdp.pdf");
    }

    struct FakePdfDialogs {
        disclosure: bool,
        choice: PreviewChoice,
        done: bool,
        path: Option<std::path::PathBuf>,
        fail_preview: bool,
        calls: Vec<&'static str>,
        preview_bytes: Vec<u8>,
    }

    impl PdfDialogs for FakePdfDialogs {
        async fn disclosure(&mut self) -> bool {
            self.calls.push("disclosure");
            self.disclosure
        }
        async fn preview_choice(&mut self) -> PreviewChoice {
            self.calls.push("choice");
            self.choice
        }
        fn open_preview(&mut self, bytes: &[u8]) -> anyhow::Result<()> {
            self.calls.push("preview");
            self.preview_bytes = bytes.to_vec();
            if self.fail_preview {
                anyhow::bail!("preview failed");
            }
            Ok(())
        }
        async fn preview_done(&mut self) -> bool {
            self.calls.push("done");
            self.done
        }
        async fn destination(&mut self, _: bool) -> Option<std::path::PathBuf> {
            self.calls.push("destination");
            self.path.clone()
        }
    }

    fn fake_dialogs() -> FakePdfDialogs {
        FakePdfDialogs {
            disclosure: true,
            choice: PreviewChoice::Skip,
            done: true,
            path: Some("output.pdf".into()),
            fail_preview: false,
            calls: Vec::new(),
            preview_bytes: Vec::new(),
        }
    }

    #[test]
    fn pdf_dialog_cancellation_never_reaches_save() {
        for (disclosure, choice, done, path, expected) in [
            (false, PreviewChoice::Skip, true, true, vec!["disclosure"]),
            (
                true,
                PreviewChoice::Cancel,
                true,
                true,
                vec!["disclosure", "choice"],
            ),
            (
                true,
                PreviewChoice::Preview,
                false,
                true,
                vec!["disclosure", "choice", "preview", "done"],
            ),
            (
                true,
                PreviewChoice::Skip,
                true,
                false,
                vec!["disclosure", "choice", "destination"],
            ),
        ] {
            let mut fake = fake_dialogs();
            fake.disclosure = disclosure;
            fake.choice = choice;
            fake.done = done;
            if !path {
                fake.path = None;
            }
            let renders = std::cell::Cell::new(0);
            let result =
                futures::executor::block_on(prepare_pdf_export(&mut fake, false, || async {
                    renders.set(renders.get() + 1);
                    Ok(b"pdf bytes".to_vec())
                }))
                .unwrap();
            assert!(result.is_none());
            assert_eq!(fake.calls, expected);
            assert_eq!(renders.get(), usize::from(disclosure));
        }
    }

    #[test]
    fn pdf_dialog_preview_shows_the_exact_bytes_that_will_be_saved() {
        let mut fake = fake_dialogs();
        fake.choice = PreviewChoice::Preview;
        let (path, bytes) =
            futures::executor::block_on(prepare_pdf_export(&mut fake, true, || async {
                Ok(b"exact snapshot".to_vec())
            }))
            .unwrap()
            .unwrap();
        assert_eq!(path, std::path::PathBuf::from("output.pdf"));
        assert_eq!(fake.preview_bytes, bytes);
        assert_eq!(
            fake.calls,
            ["disclosure", "choice", "preview", "done", "destination"]
        );
    }

    #[test]
    fn pdf_preview_file_contains_the_exact_bytes_and_is_removed() {
        let (dir, path) = create_pdf_preview(b"pdf snapshot").unwrap();
        assert_eq!(std::fs::read(&path).unwrap(), b"pdf snapshot");
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt as _;
            assert_eq!(
                std::fs::metadata(dir.path()).unwrap().permissions().mode() & 0o077,
                0
            );
        }
        drop(dir);
        assert!(!path.exists());
    }

    #[test]
    fn pdf_dialog_render_failure_stops_before_preview() {
        let mut fake = fake_dialogs();
        let result = futures::executor::block_on(prepare_pdf_export(&mut fake, false, || async {
            anyhow::bail!("render failed")
        }));
        assert!(result.is_err());
        assert_eq!(fake.calls, ["disclosure"]);
    }

    #[test]
    fn pdf_dialog_preview_failure_stops_before_destination() {
        let mut fake = fake_dialogs();
        fake.choice = PreviewChoice::Preview;
        fake.fail_preview = true;
        let result = futures::executor::block_on(prepare_pdf_export(&mut fake, false, || async {
            Ok(b"pdf".to_vec())
        }));
        assert!(result.is_err());
        assert_eq!(fake.calls, ["disclosure", "choice", "preview"]);
    }

    fn render_pdf_text(body: String) -> anyhow::Result<Vec<u8>> {
        render_pdf(vec![crate::transcript::PdfSection {
            markdown: body,
            images: Vec::new(),
        }])
    }

    fn test_image(format: image::ImageFormat, width: u32) -> std::sync::Arc<gpui::Image> {
        let raster = image::RgbImage::from_pixel(width, 2, image::Rgb([255, 0, 0]));
        let mut bytes = std::io::Cursor::new(Vec::new());
        image::DynamicImage::ImageRgb8(raster)
            .write_to(&mut bytes, format)
            .unwrap();
        std::sync::Arc::new(gpui::Image::from_bytes(
            match format {
                image::ImageFormat::Jpeg => gpui::ImageFormat::Jpeg,
                image::ImageFormat::Gif => gpui::ImageFormat::Gif,
                image::ImageFormat::Bmp => gpui::ImageFormat::Bmp,
                image::ImageFormat::WebP => gpui::ImageFormat::Webp,
                _ => gpui::ImageFormat::Png,
            },
            bytes.into_inner(),
        ))
    }

    fn image_count(doc: &lopdf::Document) -> usize {
        doc.objects
            .values()
            .filter(|object| {
                object
                    .as_stream()
                    .ok()
                    .and_then(|stream| stream.dict.get(b"Subtype").ok())
                    .and_then(|kind| kind.as_name().ok())
                    == Some(b"Image".as_slice())
            })
            .count()
    }

    fn plain() -> crate::transcript::ExportOptions {
        crate::transcript::ExportOptions::default()
    }

    fn with_work() -> crate::transcript::ExportOptions {
        crate::transcript::ExportOptions {
            include_thinking_and_tools: true,
        }
    }

    #[test]
    fn pdf_export_snapshots_and_renders_attached_png_and_jpeg() {
        let mut transcript = crate::transcript::Transcript::new();
        let images = vec![
            test_image(image::ImageFormat::Png, 2),
            test_image(image::ImageFormat::Jpeg, 3),
        ];
        transcript.append_user_message("See attachments", images);
        let snapshot = transcript.export_visible_pdf(plain()).unwrap();
        transcript.clear();
        let doc = lopdf::Document::load_mem(&render_pdf(snapshot).unwrap()).unwrap();
        assert_eq!(image_count(&doc), 2);
        assert!(doc.extract_text(&[1]).unwrap().contains("See attachments"));
    }

    /// The opt-in appendix must reach the PDF too, not only the Markdown file.
    #[test]
    fn pdf_export_renders_the_reasoning_appendix_when_opted_in() {
        let mut transcript = crate::transcript::Transcript::new();
        transcript.load_from(&serde_json::json!({"messages": [
            {"role": "assistant", "content": [
                {"type": "thinking", "thinking": "weighed the options"},
                {"type": "text", "text": "Answer"}
            ]}
        ]}));
        let section = transcript.export_last_response_pdf(with_work()).unwrap();
        transcript.clear();
        let doc = lopdf::Document::load_mem(&render_pdf(vec![section]).unwrap()).unwrap();
        let text = doc.extract_text(&[1]).unwrap();
        assert!(text.contains("Answer"), "{text:?}");
        assert!(text.contains("Reasoning"), "{text:?}");
        assert!(text.contains("weighed the options"), "{text:?}");
    }

    /// A long conversation must render past the first page — the large-session
    /// case the plan left open.
    #[test]
    fn pdf_export_renders_a_large_session_across_pages() {
        let mut messages = Vec::new();
        for turn in 0..120 {
            messages.push(serde_json::json!({
                "role": "user",
                "content": format!("question {turn}")
            }));
            messages.push(serde_json::json!({
                "role": "assistant",
                "content": [{
                    "type": "text",
                    "text": format!("answer {turn}\n\n{}", "detail ".repeat(24))
                }]
            }));
        }
        let mut transcript = crate::transcript::Transcript::new();
        transcript.load_from(&serde_json::json!({ "messages": messages }));
        let sections = transcript.export_visible_pdf(plain()).unwrap();
        assert_eq!(sections.len(), 240);
        let doc = lopdf::Document::load_mem(&render_pdf(sections).unwrap()).unwrap();
        assert!(doc.get_pages().len() > 1, "a long session must paginate");
        let pages: Vec<String> = doc
            .get_pages()
            .keys()
            .map(|page| doc.extract_text(&[*page]).unwrap_or_default())
            .collect();
        assert!(pages.iter().any(|page| page.contains("question 0")));
        assert!(pages.iter().any(|page| page.contains("question 119")));
    }

    #[test]
    fn pdf_export_snapshot_targets_the_selected_response() {
        use base64::Engine as _;
        let image = test_image(image::ImageFormat::Png, 2);
        let data = base64::engine::general_purpose::STANDARD.encode(&image.bytes);
        let mut transcript = crate::transcript::Transcript::new();
        transcript.load_from(&serde_json::json!({"messages": [
            {"role": "assistant", "content": [{"type": "text", "text": "first"}, {"type": "image", "mimeType": "image/png", "data": data}]},
            {"role": "user", "content": "next"},
            {"role": "assistant", "content": "second"}
        ]}));
        let selected = transcript.export_response_pdf(0, "first", plain()).unwrap();
        assert!(transcript
            .export_response_pdf(0, "second", plain())
            .is_none());
        assert!(transcript.export_response_pdf(1, "next", plain()).is_none());
        assert!(transcript
            .export_last_response_pdf(plain())
            .unwrap()
            .images
            .is_empty());
        transcript.clear();
        let doc = lopdf::Document::load_mem(&render_pdf(vec![selected]).unwrap()).unwrap();
        assert_eq!(image_count(&doc), 1);
        assert!(doc.extract_text(&[1]).unwrap().contains("first"));
    }

    #[test]
    fn pdf_export_converts_other_supported_raster_formats() {
        let images = vec![
            test_image(image::ImageFormat::Gif, 2),
            test_image(image::ImageFormat::Bmp, 3),
            test_image(image::ImageFormat::WebP, 4),
        ];
        let section = crate::transcript::PdfSection {
            markdown: "images".into(),
            images,
        };
        let doc = lopdf::Document::load_mem(&render_pdf(vec![section]).unwrap()).unwrap();
        assert_eq!(image_count(&doc), 3);
    }

    #[test]
    fn pdf_export_with_attachments_blocks_local_traversal_and_remote_images() {
        let dir = tempfile::tempdir().unwrap();
        let secret = dir.path().join("secret.png");
        image::RgbImage::from_pixel(5, 5, image::Rgb([0, 255, 0]))
            .save(&secret)
            .unwrap();
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        listener.set_nonblocking(true).unwrap();
        let body = format!("![absolute]({})\n\n![traversal](../../../../../../{})\n\n![remote](http://{}/secret.png)", secret.display(), secret.display().to_string().trim_start_matches('/'), listener.local_addr().unwrap());
        let sections = vec![crate::transcript::PdfSection {
            markdown: body,
            images: vec![test_image(image::ImageFormat::Png, 2)],
        }];
        let doc = lopdf::Document::load_mem(&render_pdf(sections).unwrap()).unwrap();
        assert_eq!(
            image_count(&doc),
            1,
            "only the attached image may be embedded"
        );
        assert_eq!(
            listener.accept().unwrap_err().kind(),
            std::io::ErrorKind::WouldBlock
        );
    }

    #[test]
    fn pdf_export_rejects_invalid_or_oversized_attachments() {
        for image in [
            std::sync::Arc::new(gpui::Image::from_bytes(
                gpui::ImageFormat::Png,
                b"invalid".to_vec(),
            )),
            test_image(image::ImageFormat::Png, 4001),
        ] {
            let section = crate::transcript::PdfSection {
                markdown: "hello".into(),
                images: vec![image],
            };
            assert!(render_pdf(vec![section]).is_err());
        }
    }

    #[test]
    fn pdf_export_renders_unicode_tables_and_paginated_code() {
        let markdown = format!(
            "# Résumé\n\nCafé — hello\n\n| A | B |\n|---|---|\n| one | two |\n\n```rust\n{}\n```",
            "let value = 42;\n".repeat(180)
        );
        let bytes = render_pdf_text(markdown).unwrap();
        let doc = lopdf::Document::load_mem(&bytes).unwrap();
        assert!(doc.get_pages().len() > 1);
        let text = doc.extract_text(&[1]).unwrap();
        assert!(text.contains("Résumé"), "{text:?}");
        assert!(text.contains("Café"), "{text:?}");
        assert!(text.contains("one"), "{text:?}");
    }

    /// These fixtures are test-only: production relies on installed fallbacks.
    /// `markdown2pdf` currently emits one glyph per codepoint without shaping.
    #[test]
    fn pdf_complex_scripts_preserve_text_but_do_not_apply_required_shaping() {
        use markdown2pdf::fonts::FontSource;
        const ARABIC: &[u8] =
            include_bytes!("../../tests/fixtures/pdf-fonts/NotoSansArabic-Regular.ttf");
        const DEVANAGARI: &[u8] =
            include_bytes!("../../tests/fixtures/pdf-fonts/NotoSansDevanagari-Regular.ttf");
        for (text, font) in [("لا", ARABIC), ("क्ष", DEVANAGARI)] {
            let fonts = pdf_fonts().add_fallback_font_source(FontSource::bytes(font));
            let section = crate::transcript::PdfSection {
                markdown: text.into(),
                images: Vec::new(),
            };
            let doc =
                lopdf::Document::load_mem(&render_pdf_with_fonts(vec![section], &fonts).unwrap())
                    .unwrap();
            assert_eq!(doc.extract_text(&[1]).unwrap().trim(), text);

            // Actual PDF text operators use two-byte CIDs for the subset font.
            let page = *doc.get_pages().values().next().unwrap();
            let content = lopdf::content::Content::decode(&doc.get_page_content(page)).unwrap();
            let glyphs: Vec<u16> = content
                .operations
                .iter()
                .filter(|op| op.operator == "Tj")
                .flat_map(|op| op.operands[0].as_str().unwrap().as_chunks::<2>().0.iter())
                .map(|cid| u16::from_be_bytes(*cid))
                .collect();
            assert!(
                glyphs.iter().all(|&glyph| glyph != 0),
                "missing glyph in {text:?}"
            );
            assert_eq!(glyphs.len(), text.chars().count());

            // A shaping engine substitutes a ligature for each sample. This
            // comparison deliberately records the renderer's current defect;
            // update the test and documentation when real shaping is added.
            let face = rustybuzz::Face::from_slice(font, 0).unwrap();
            let mut buffer = rustybuzz::UnicodeBuffer::new();
            buffer.push_str(text);
            let shaped = rustybuzz::shape(&face, &[], buffer);
            assert!(
                shaped.len() < glyphs.len(),
                "expected a shaped ligature for {text:?}"
            );
        }
    }

    #[test]
    fn pdf_arabic_is_emitted_in_logical_order_not_shaped_rtl_order() {
        use markdown2pdf::fonts::FontSource;
        const ARABIC: &[u8] =
            include_bytes!("../../tests/fixtures/pdf-fonts/NotoSansArabic-Regular.ttf");
        let text = "سلام";
        let fonts = pdf_fonts().add_fallback_font_source(FontSource::bytes(ARABIC));
        let section = crate::transcript::PdfSection {
            markdown: text.into(),
            images: Vec::new(),
        };
        let doc = lopdf::Document::load_mem(&render_pdf_with_fonts(vec![section], &fonts).unwrap())
            .unwrap();
        assert_eq!(doc.extract_text(&[1]).unwrap().trim(), text);
        let page = *doc.get_pages().values().next().unwrap();
        let content = lopdf::content::Content::decode(&doc.get_page_content(page)).unwrap();
        let strings: Vec<_> = content
            .operations
            .iter()
            .filter(|op| op.operator == "Tj")
            .collect();
        assert_eq!(
            strings.len(),
            1,
            "the renderer currently emits a single logical-order run"
        );
        assert_eq!(
            strings[0].operands[0].as_str().unwrap().len(),
            text.chars().count() * 2
        );

        // Rustybuzz produces visual RTL order and joining forms; the PDF
        // currently contains the unshaped logical run instead.
        let face = rustybuzz::Face::from_slice(ARABIC, 0).unwrap();
        let mut buffer = rustybuzz::UnicodeBuffer::new();
        buffer.push_str(text);
        let shaped = rustybuzz::shape(&face, &[], buffer);
        let clusters = shaped.glyph_infos();
        assert!(clusters.first().unwrap().cluster > clusters.last().unwrap().cluster);
        assert!(shaped.len() < text.chars().count());
    }

    #[test]
    fn pdf_export_preserves_greek_cyrillic_and_extended_latin() {
        let text = "Ελληνικά Привет Zażółć café";
        let doc = lopdf::Document::load_mem(&render_pdf_text(text.into()).unwrap()).unwrap();
        let extracted = doc.extract_text(&[1]).unwrap();
        for word in ["Ελληνικά", "Привет", "Zażółć", "café"] {
            assert!(
                extracted.contains(word),
                "missing {word:?} in {extracted:?}"
            );
        }
    }

    #[test]
    fn pdf_export_uses_system_cjk_fallback_when_available() {
        if markdown2pdf::fonts::find_system_font("Arial Unicode MS").is_none()
            && markdown2pdf::fonts::find_system_font("Noto Sans CJK SC").is_none()
        {
            return;
        }
        let doc =
            lopdf::Document::load_mem(&render_pdf_text("中文 日本語".into()).unwrap()).unwrap();
        let text = doc.extract_text(&[1]).unwrap();
        assert!(text.contains("中文"), "{text:?}");
        assert!(text.contains("日本語"), "{text:?}");
    }

    #[test]
    fn pdf_export_does_not_lose_lines_at_page_breaks() {
        let lines = (0..250)
            .map(|n| format!("LINE_{n:03} marker\n\n"))
            .collect::<String>();
        let doc = lopdf::Document::load_mem(&render_pdf_text(lines).unwrap()).unwrap();
        assert!(doc.get_pages().len() > 1);
        let pages: Vec<_> = doc.get_pages().keys().copied().collect();
        let text = doc.extract_text(&pages).unwrap();
        for n in 0..250 {
            let marker = format!("LINE_{n:03}");
            assert_eq!(
                text.matches(&marker).count(),
                1,
                "missing or repeated {marker}"
            );
        }
    }

    #[test]
    fn pdf_export_places_text_origins_inside_page_bounds() {
        let markdown = format!("# Heading\n\n{}", "A paragraph with text.\n\n".repeat(100));
        let doc = lopdf::Document::load_mem(&render_pdf_text(markdown).unwrap()).unwrap();
        let mut positions = 0;
        for (_, page) in doc.get_pages() {
            let bytes = doc.get_page_content(page);
            let content = lopdf::content::Content::decode(&bytes).unwrap();
            for op in content
                .operations
                .iter()
                .filter(|op| op.operator == "Td" || op.operator == "Tm")
            {
                let offset = if op.operator == "Tm" { 4 } else { 0 };
                let x = op.operands[offset].as_f32().unwrap();
                let y = op.operands[offset + 1].as_f32().unwrap();
                assert!((0.0..=612.0).contains(&x), "x={x}");
                assert!((0.0..=842.0).contains(&y), "y={y}");
                positions += 1;
            }
        }
        assert!(positions > 0);
    }

    #[test]
    fn pdf_export_keeps_layout_consistent_across_renders() {
        let body = format!("# Heading\n\n{}", "Paragraph with text.\n\n".repeat(150));
        let render = || {
            let doc = lopdf::Document::load_mem(&render_pdf_text(body.clone()).unwrap()).unwrap();
            doc.get_pages()
                .keys()
                .map(|page| doc.extract_text(&[*page]).unwrap())
                .collect::<Vec<_>>()
        };
        assert_eq!(render(), render());
    }

    #[test]
    fn pdf_export_preserves_frontmatter_and_long_code_lines() {
        let body = format!(
            "---\ntitle: Keep this text\n---\n\n```\n{} END_OF_LINE\n```",
            "word ".repeat(400)
        );
        let bytes = render_pdf_text(body).unwrap();
        let doc = lopdf::Document::load_mem(&bytes).unwrap();
        let pages: Vec<_> = doc.get_pages().keys().copied().collect();
        let text = doc.extract_text(&pages).unwrap();
        assert!(text.contains("Keep this text"), "{text:?}");
        assert!(text.contains("END_OF_LINE"), "{text:?}");
    }

    #[test]
    fn pdf_export_does_not_read_local_image_paths() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("secret.png");
        image::RgbImage::from_pixel(1, 1, image::Rgb([255, 0, 0]))
            .save(&path)
            .unwrap();
        let body = format!("# image\n\n![secret]({})", path.display());
        let bytes = render_pdf_text(body).unwrap();
        let doc = lopdf::Document::load_mem(&bytes).unwrap();
        assert!(!doc.objects.values().any(|object| {
            object
                .as_stream()
                .ok()
                .and_then(|stream| stream.dict.get(b"Subtype").ok())
                .and_then(|kind| kind.as_name().ok())
                == Some(b"Image".as_slice())
        }));
    }

    #[test]
    fn pdf_export_never_overwrites() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("existing.pdf");
        std::fs::write(&path, b"keep").unwrap();
        assert_eq!(
            write_new_pdf(&path, b"replacement").unwrap_err().kind(),
            std::io::ErrorKind::AlreadyExists
        );
        assert_eq!(std::fs::read(path).unwrap(), b"keep");
    }

    #[test]
    fn markdown_export_reports_an_invalid_destination() {
        let dir = std::env::temp_dir().join(format!("orbit-export-missing-{}", std::process::id()));
        let error = write_new_markdown(&dir.join("missing").join("export.md"), "text").unwrap_err();
        assert_eq!(error.kind(), std::io::ErrorKind::NotFound);
    }

    #[test]
    fn markdown_export_writes_large_unicode_content() {
        let dir = std::env::temp_dir().join(format!("orbit-export-large-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("export.md");
        let body = "🪐 large transcript\n".repeat(100_000);
        write_new_markdown(&path, &body).unwrap();
        assert_eq!(std::fs::read_to_string(path).unwrap(), body);
        std::fs::remove_dir_all(dir).unwrap();
    }

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
