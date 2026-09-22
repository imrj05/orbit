//! File loading for the Explorer's viewer.
//!
//! Split in two: [`load`] is pure filesystem work (no GPUI), so it runs on the
//! background executor and is unit-tested without a window; the `FileViewer`
//! entity further down only paints what `load` returns.
//!
//! Guards are deliberate and honest: a directory, a binary file, or a file
//! over [`MAX_FILE_BYTES`] is reported as such — never dumped lossily at the
//! user or read into memory unbounded.

use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::sync::Arc;
use std::time::Duration;

use gpui::{
    div, img, prelude::*, px, Animation, AnimationExt, AnyElement, App, ClickEvent, Context,
    FocusHandle, Font, FontFeatures, FontStyle, FontWeight, Hsla, Image, ImageFormat, ImageSource,
    ListAlignment, ListState, ObjectFit, Render, ScrollHandle, StyledText, TextAlign, TextRun,
    Transformation, Window,
};

use crate::app::{file_badge, file_glyph, icon, nerd_font_family};
use crate::highlight::{self, Lang, Token};
use crate::theme::{self, Theme, ThemeMode};

/// Files above this are not previewed. Two megabytes is far past any source
/// file a person reads and well under anything that would stall a frame.
pub const MAX_FILE_BYTES: u64 = 2 * 1024 * 1024;

/// A line cap for pathological generated files that are still under the byte
/// cap (minified bundles). Long past anything readable.
pub const MAX_LINES: usize = 200_000;

/// How many leading bytes to sniff for a NUL when deciding "binary".
const BINARY_SNIFF_BYTES: usize = 8_000;

/// How a loaded file should be painted.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Mode {
    Code { lang: Lang },
    Markdown,
    Text,
    Image,
    Binary,
    TooLarge,
}

/// A file ready to paint. `lines`/`tokens` are empty for the modes that do not
/// render text.
#[derive(Clone, Debug)]
pub struct FileContent {
    pub path: PathBuf,
    /// Workspace-relative path for the header, when known.
    pub display: String,
    pub mode: Mode,
    pub lines: Vec<String>,
    /// Paint-only syntax spans over `lines`, computed off the UI thread.
    /// `None` when the mode is not [`Mode::Code`].
    pub tokens: Option<Vec<Vec<Token>>>,
    /// Raw bytes for [`Mode::Image`], already bounded by [`MAX_FILE_BYTES`].
    pub image_bytes: Option<Vec<u8>>,
    pub bytes: u64,
    /// The line cap trimmed the document.
    pub truncated: bool,
    /// A read error worth showing in place of the content.
    pub error: Option<String>,
}

impl FileContent {
    fn failed(path: PathBuf, display: String, error: impl Into<String>) -> Self {
        Self {
            path,
            display,
            mode: Mode::Text,
            lines: Vec::new(),
            tokens: None,
            image_bytes: None,
            bytes: 0,
            truncated: false,
            error: Some(error.into()),
        }
    }

    pub fn line_count(&self) -> usize {
        self.lines.len()
    }
}

/// Load a file for preview. Never returns `Err`: failures become a
/// [`FileContent`] carrying [`FileContent::error`], so the viewer always has
/// something honest to render.
pub fn load(path: &Path, display: impl Into<String>) -> FileContent {
    let display = display.into();
    let metadata = match std::fs::metadata(path) {
        Ok(metadata) => metadata,
        Err(error) => return FileContent::failed(path.to_path_buf(), display, error.to_string()),
    };
    if metadata.is_dir() {
        return FileContent::failed(path.to_path_buf(), display, "not a file");
    }

    let bytes = metadata.len();
    if bytes > MAX_FILE_BYTES {
        return FileContent {
            path: path.to_path_buf(),
            display,
            mode: Mode::TooLarge,
            lines: Vec::new(),
            tokens: None,
            image_bytes: None,
            bytes,
            truncated: false,
            error: None,
        };
    }

    let Ok(raw) = std::fs::read(path) else {
        return FileContent::failed(path.to_path_buf(), display, "could not read file");
    };

    let name = display.rsplit('/').next().unwrap_or(&display);
    if is_image(name) {
        return FileContent {
            path: path.to_path_buf(),
            display,
            mode: Mode::Image,
            lines: Vec::new(),
            tokens: None,
            image_bytes: Some(raw),
            bytes,
            truncated: false,
            error: None,
        };
    }

    if is_binary(&raw) {
        return FileContent {
            path: path.to_path_buf(),
            display,
            mode: Mode::Binary,
            lines: Vec::new(),
            tokens: None,
            image_bytes: None,
            bytes,
            truncated: false,
            error: None,
        };
    }

    let text = String::from_utf8_lossy(&raw);
    let mut lines: Vec<String> = text.split('\n').map(|line| line.to_string()).collect();
    // `split` yields a trailing empty line for a file ending in a newline;
    // drop it so the gutter does not show a phantom last line.
    if lines.last().is_some_and(String::is_empty) && text.ends_with('\n') {
        lines.pop();
    }
    let truncated = lines.len() > MAX_LINES;
    if truncated {
        lines.truncate(MAX_LINES);
    }

    let (mode, tokens) = if is_markdown(name) {
        (Mode::Markdown, None)
    } else if let Some(lang) = crate::review::language_for_path(name) {
        (Mode::Code { lang }, Some(highlight::tokenize(lang, &text)))
    } else {
        (Mode::Text, None)
    };

    FileContent {
        path: path.to_path_buf(),
        display,
        mode,
        lines,
        tokens,
        image_bytes: None,
        bytes,
        truncated,
        error: None,
    }
}

fn extension(path: &str) -> Option<String> {
    Path::new(path)
        .extension()
        .and_then(|ext| ext.to_str())
        .map(|ext| ext.to_ascii_lowercase())
}

pub fn is_markdown(path: &str) -> bool {
    matches!(extension(path).as_deref(), Some("md" | "markdown" | "mdx"))
}

pub fn is_image(path: &str) -> bool {
    matches!(
        extension(path).as_deref(),
        Some("png" | "jpg" | "jpeg" | "gif" | "webp" | "bmp" | "tiff" | "svg")
    )
}

/// The GPUI decoder for an image path, if any. `.ico`/`.avif` are recognized
/// by consumers but not decodable by gpui, so they fall through to the binary
/// notice rather than failing a decode.
fn image_format(path: &Path) -> Option<ImageFormat> {
    let name = path
        .file_name()
        .and_then(|name| name.to_str())
        .map(|name| name.to_ascii_lowercase())?;
    match extension(&name).as_deref() {
        Some("png") => Some(ImageFormat::Png),
        Some("jpg" | "jpeg") => Some(ImageFormat::Jpeg),
        Some("gif") => Some(ImageFormat::Gif),
        Some("webp") => Some(ImageFormat::Webp),
        Some("bmp") => Some(ImageFormat::Bmp),
        Some("tiff") => Some(ImageFormat::Tiff),
        Some("svg") => Some(ImageFormat::Svg),
        _ => None,
    }
}

/// A NUL byte in the leading window is the classic binary tell and is right
/// for source trees; a stray high byte is not enough to call something binary.
fn is_binary(bytes: &[u8]) -> bool {
    bytes.iter().take(BINARY_SNIFF_BYTES).any(|byte| *byte == 0)
}

// ── the GPUI entity ────────────────────────────────────────────────────

struct Tab {
    path: PathBuf,
    display: String,
}

/// The full-page Files surface: a tab strip over a read-only file view.
///
/// Loading always happens on the background executor; the entity only ever
/// paints an already-loaded [`FileContent`].
pub struct FileViewer {
    open: bool,
    tabs: Vec<Tab>,
    active: usize,
    content: Option<FileContent>,
    loading: bool,
    /// Guards a late load from a previous request.
    generation: u64,
    list: ListState,
    /// Decoded image for the active file, when it is an image.
    image: Option<Arc<Image>>,
    /// Leading inset the tab strip gives its first tab. The surface spans the
    /// window when the sessions sidebar is collapsed, so this clears the macOS
    /// traffic lights and the sidebar/history controls overlaid in the
    /// titlebar (mirrors the Git/Usage page headers).
    chrome_leading: f32,
    /// Reserve the top-right caption-controls inset (Windows only, and only
    /// while this surface is the window's rightmost column).
    reserve_controls: bool,
    /// Horizontal scroll of the tab strip, so a long run of open files stays
    /// reachable instead of being clipped off the right edge.
    tab_scroll: ScrollHandle,
    /// Carries the `Files` key context so `cmd-w` closes the surface.
    focus: FocusHandle,
    /// Focus the surface on the next paint (`show` has no window).
    focus_pending: bool,
    /// The app's close callback (the toolbar's X).
    on_close: Rc<dyn Fn(&mut App)>,
}

impl FileViewer {
    pub fn new(on_close: Rc<dyn Fn(&mut App)>, cx: &mut Context<Self>) -> Self {
        Self {
            open: false,
            tabs: Vec::new(),
            active: 0,
            content: None,
            loading: false,
            generation: 0,
            list: ListState::new(0, ListAlignment::Top, px(400.)),
            image: None,
            chrome_leading: 12.,
            reserve_controls: false,
            tab_scroll: ScrollHandle::new(),
            focus: cx.focus_handle(),
            focus_pending: false,
            on_close,
        }
    }

    /// The app syncs this each render with the page leading: with the sidebar
    /// open the surface starts after it and only needs the normal page padding;
    /// collapsed, it owns the window's left edge and must clear the overlaid
    /// titlebar controls.
    pub fn set_chrome_leading(&mut self, leading: f32, cx: &mut Context<Self>) {
        if (self.chrome_leading - leading).abs() > 0.5 {
            self.chrome_leading = leading;
            cx.notify();
        }
    }

    /// Reserve the window-control inset in the tab strip (Windows only, and
    /// only while this surface is the window's rightmost column).
    pub fn set_reserve_controls(&mut self, reserve: bool, cx: &mut Context<Self>) {
        if reserve != self.reserve_controls {
            self.reserve_controls = reserve;
            cx.notify();
        }
    }

    pub fn is_open(&self) -> bool {
        self.open
    }

    pub fn active_path(&self) -> Option<PathBuf> {
        self.tabs.get(self.active).map(|tab| tab.path.clone())
    }

    pub fn active_display(&self) -> Option<String> {
        self.tabs.get(self.active).map(|tab| tab.display.clone())
    }

    /// Open (or focus) a file and load it.
    pub fn show(&mut self, path: PathBuf, display: String, cx: &mut Context<Self>) {
        self.open = true;
        self.focus_pending = true;
        let index = match self.tabs.iter().position(|tab| tab.path == path) {
            Some(index) => index,
            None => {
                self.tabs.push(Tab {
                    path: path.clone(),
                    display: display.clone(),
                });
                self.tabs.len() - 1
            }
        };
        self.active = index;
        self.read(path, display, cx);
        // Reveal the tab the user just opened if the strip has scrolled.
        self.tab_scroll.scroll_to_item(self.active);
        cx.notify();
    }

    pub fn hide(&mut self, cx: &mut Context<Self>) {
        self.open = false;
        cx.notify();
    }

    pub fn close_tab(&mut self, index: usize, cx: &mut Context<Self>) {
        if index >= self.tabs.len() {
            return;
        }
        self.tabs.remove(index);
        if self.tabs.is_empty() {
            self.open = false;
            self.content = None;
            self.image = None;
            self.list.reset(0);
        } else {
            self.active = self.active.min(self.tabs.len() - 1);
            let (path, display) = self.tab_target(cx);
            self.read(path, display, cx);
            self.tab_scroll.scroll_to_item(self.active);
        }
        cx.notify();
    }

    /// The workspace watcher fired: reload the active file when it changed.
    pub fn reload(&mut self, path: &Path, cx: &mut Context<Self>) {
        let Some(tab) = self.tabs.get(self.active) else {
            return;
        };
        if tab.path != path {
            return;
        }
        let (path, display) = self.tab_target(cx);
        self.read(path, display, cx);
    }

    fn tab_target(&self, _cx: &Context<Self>) -> (PathBuf, String) {
        let tab = &self.tabs[self.active];
        (tab.path.clone(), tab.display.clone())
    }

    fn read(&mut self, path: PathBuf, display: String, cx: &mut Context<Self>) {
        self.loading = true;
        self.generation = self.generation.wrapping_add(1);
        let generation = self.generation;
        self.content = None;
        self.image = None;
        self.list.reset(0);
        cx.spawn(async move |this, cx| {
            let content = cx
                .background_executor()
                .spawn(async move { load(&path, display) })
                .await;
            let _ = this.update(cx, |this, cx| {
                if this.generation != generation {
                    return;
                }
                this.image = if content.mode == Mode::Image {
                    content.image_bytes.as_deref().and_then(|bytes| {
                        image_format(&content.path)
                            .map(|format| Arc::new(Image::from_bytes(format, bytes.to_vec())))
                    })
                } else {
                    None
                };
                let count = content.lines.len();
                if count != this.list.item_count() {
                    this.list.reset(count);
                }
                this.loading = false;
                this.content = Some(content);
                cx.notify();
            });
        })
        .detach();
    }

    // ── rendering ──────────────────────────────────────────────────────

    fn tab_strip(&self, theme: Theme, cx: &mut Context<Self>) -> AnyElement {
        // The tabs live in their own horizontally scrollable row: a long run of
        // open files stays reachable (trackpad / shift-wheel) instead of
        // clipping off the right edge. The close button stays pinned outside it.
        let mut tabs = div()
            .id("viewer-tabs")
            .h_full()
            .flex()
            .items_center()
            .gap(theme.space(6.))
            .overflow_x_scroll()
            .track_scroll(&self.tab_scroll);
        for (index, tab) in self.tabs.iter().enumerate() {
            let active = index == self.active;
            let name = tab
                .display
                .rsplit('/')
                .next()
                .unwrap_or(&tab.display)
                .to_string();
            let nerd = nerd_font_family(cx);
            let dark = theme.mode == ThemeMode::Dark;
            let fallback = file_badge(&tab.display, theme);
            let glyph = file_glyph(&tab.display, dark, nerd.as_ref(), 12., fallback);
            tabs = tabs.child(
                div()
                    .id(gpui::ElementId::Name(format!("viewer-tab-{index}").into()))
                    .h(px(28.))
                    .max_w(px(220.))
                    // Never shrink below a readable width: past the strip's
                    // edge the row scrolls instead of crushing every tab.
                    .flex_none()
                    .px(px(10.))
                    .flex()
                    .items_center()
                    .gap(px(6.))
                    .rounded(px(8.))
                    .cursor_pointer()
                    .text_size(theme.ui_px(12.5))
                    .when(active, |el| el.bg(theme.active))
                    .when(!active, |el| {
                        el.text_color(theme.text_3)
                            .hover(|el| el.bg(theme.bg_hover).text_color(theme.text_2))
                    })
                    .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                        this.active = index;
                        let (path, display) = this.tab_target(cx);
                        this.read(path, display, cx);
                        this.tab_scroll.scroll_to_item(index);
                        cx.notify();
                    }))
                    .child(glyph)
                    .child(
                        div()
                            .min_w_0()
                            .truncate()
                            .font_weight(if active {
                                FontWeight::MEDIUM
                            } else {
                                FontWeight::NORMAL
                            })
                            .text_color(if active {
                                theme.active_fg
                            } else {
                                theme.text_3
                            })
                            .child(name),
                    )
                    .child(
                        div()
                            .id(gpui::ElementId::Name(
                                format!("viewer-tab-close-{index}").into(),
                            ))
                            .size(px(16.))
                            .flex_none()
                            .rounded(px(4.))
                            .flex()
                            .items_center()
                            .justify_center()
                            .cursor_pointer()
                            .hover(|el| el.bg(theme.overlay))
                            .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                                this.close_tab(index, cx);
                                // Click handlers bubble: without this the tab's
                                // own `on_click` below would also run with the
                                // now-stale `index` and index past the shrunken
                                // tab list (out-of-bounds panic).
                                cx.stop_propagation();
                            }))
                            .child(icon(
                                "icons/x.svg",
                                10.,
                                if active {
                                    theme.active_fg
                                } else {
                                    theme.text_3
                                },
                            )),
                    ),
            );
        }
        div()
            .h(px(40.))
            .flex_none()
            .flex()
            .items_center()
            .gap(theme.space(6.))
            // The surface spans the window when the sidebar is collapsed, so
            // the leading inset keeps the first tab clear of the macOS traffic
            // lights and the overlaid titlebar controls; the trailing inset
            // clears the app's own caption buttons where it draws them.
            .pl(px(self.chrome_leading))
            .pr(px(if self.reserve_controls {
                crate::platform::WINDOW_CONTROLS_W
            } else {
                f32::from(theme.space(16.))
            }))
            .border_b_1()
            .border_color(theme.border)
            .child(tabs.flex_1().min_w_0())
            .child(
                div()
                    .id("viewer-close")
                    .size(px(24.))
                    .flex_none()
                    .rounded(px(6.))
                    .flex()
                    .items_center()
                    .justify_center()
                    .cursor_pointer()
                    .hover(|el| el.bg(theme.bg_hover))
                    .on_click(cx.listener(|this, _: &ClickEvent, _, cx| {
                        // Close here — this listener already holds the viewer's
                        // lease, so the app callback must not re-enter `update`
                        // on this entity (that would double-lease and abort).
                        let on_close = this.on_close.clone();
                        this.hide(cx);
                        on_close(cx);
                    }))
                    .child(icon("icons/x.svg", 13., theme.text_3)),
            )
            .into_any_element()
    }

    fn toolbar(&self, theme: Theme, _cx: &mut Context<Self>) -> AnyElement {
        let Some(content) = self.content.as_ref() else {
            return div().h(px(34.)).flex_none().into_any_element();
        };
        let size = format_bytes(content.bytes);
        let lines = tr!("explorer.lines", count = content.line_count());
        div()
            .h(px(34.))
            .flex_none()
            .px(theme.space(16.))
            .flex()
            .items_center()
            .gap(px(12.))
            .border_b_1()
            .border_color(theme.border)
            .font_family(theme::code_font_family())
            .text_size(theme.ui_px(11.5))
            .text_color(theme.text_3)
            .child(
                div()
                    .min_w_0()
                    .flex_1()
                    .truncate()
                    .text_color(theme.text_2)
                    .child(content.display.clone()),
            )
            .child(div().flex_none().child(size))
            .when(
                content.mode != Mode::Binary && content.mode != Mode::TooLarge,
                |el| el.child(div().flex_none().child(lines)),
            )
            .when(content.truncated, |el| {
                el.child(
                    div()
                        .flex_none()
                        .text_color(theme.warn)
                        .child(tr!("explorer.truncated")),
                )
            })
            .child(
                div()
                    .flex_none()
                    .h(px(20.))
                    .px(px(6.))
                    .rounded(px(6.))
                    .bg(theme.bg_raised)
                    .border_1()
                    .border_color(theme.border)
                    .font_family(theme::ui_font_family())
                    .flex()
                    .items_center()
                    .gap(px(4.))
                    .child(icon("icons/lock.svg", 10., theme.text_3))
                    .child(tr!("explorer.read_only")),
            )
            .into_any_element()
    }

    fn body(&self, theme: Theme, cx: &mut Context<Self>) -> AnyElement {
        if self.loading {
            return loading_state(&theme);
        }
        let Some(content) = self.content.as_ref() else {
            let detail = tr!("explorer.select_file_detail");
            return centered_message(&theme, tr!("explorer.select_file"), Some(detail.as_str()));
        };
        if let Some(error) = &content.error {
            return centered_message(&theme, tr!("explorer.read_error"), Some(error.as_str()));
        }
        if matches!(content.mode, Mode::Code { .. } | Mode::Text) && content.lines.is_empty() {
            return centered_message(&theme, tr!("explorer.empty_file"), None);
        }
        match content.mode {
            Mode::Code { .. } | Mode::Text => self.text_body(theme, cx),
            Mode::Markdown => {
                let text = content.lines.join("\n");
                div()
                    .id("viewer-markdown")
                    .flex_1()
                    .min_h_0()
                    .overflow_y_scroll()
                    .child(
                        div()
                            .w_full()
                            .max_w(px(760.))
                            .mx_auto()
                            .p(theme.space(20.))
                            .child(crate::transcript_view::render_markdown_document(
                                &text, theme,
                            )),
                    )
                    .into_any_element()
            }
            Mode::Image => match self.image.clone() {
                Some(image) => div()
                    .flex_1()
                    .min_h_0()
                    .overflow_hidden()
                    .flex()
                    .items_center()
                    .justify_center()
                    .p(theme.space(24.))
                    .child(
                        img(ImageSource::Image(image))
                            .max_w_full()
                            .object_fit(ObjectFit::Contain),
                    )
                    .into_any_element(),
                None => centered_message(&theme, tr!("explorer.binary"), None),
            },
            Mode::Binary => {
                let detail = tr!("explorer.binary_detail");
                centered_message(&theme, tr!("explorer.binary"), Some(detail.as_str()))
            }
            Mode::TooLarge => {
                let detail = tr!("explorer.too_large", size = format_bytes(content.bytes));
                centered_message(
                    &theme,
                    tr!("explorer.too_large_title"),
                    Some(detail.as_str()),
                )
            }
        }
    }

    fn text_body(&self, theme: Theme, cx: &mut Context<Self>) -> AnyElement {
        let entity = cx.entity().downgrade();
        let list = gpui::list(self.list.clone(), move |index, _window, cx| {
            entity
                .upgrade()
                .map(|entity| entity.update(cx, |this, cx| this.render_line(index, cx)))
                .unwrap_or_else(|| div().into_any_element())
        })
        .size_full();
        div()
            .flex_1()
            .min_h_0()
            .min_w_0()
            .overflow_hidden()
            .font_family(theme::code_font_family())
            .text_size(theme.code_px(12.5))
            .line_height(theme.code_px(18.))
            .child(list)
            .into_any_element()
    }

    fn render_line(&self, index: usize, cx: &mut Context<Self>) -> AnyElement {
        let Some(content) = self.content.as_ref() else {
            return div().into_any_element();
        };
        let Some(line) = content.lines.get(index) else {
            return div().into_any_element();
        };
        let theme = *theme::get(cx);
        let tokens = content
            .tokens
            .as_ref()
            .and_then(|tokens| tokens.get(index))
            .map(Vec::as_slice)
            .unwrap_or(&[]);
        let gutter_width = content.lines.len().to_string().len().max(3) as f32 * 7.0 + 16.0;
        div()
            .w_full()
            .min_w_0()
            .flex()
            .items_start()
            .child(
                div()
                    .w(px(gutter_width))
                    .flex_none()
                    .pr(px(9.))
                    .flex()
                    .justify_end()
                    .border_r_1()
                    .border_color(theme.border)
                    .text_color(theme.text_3.opacity(0.7))
                    .child((index + 1).to_string()),
            )
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .pl(px(10.))
                    .pr(px(12.))
                    .overflow_hidden()
                    .whitespace_nowrap()
                    .child(line_text(line, tokens, theme)),
            )
            .into_any_element()
    }
}

/// A page-level spinner; static under reduce-motion.
fn spinner(id: &'static str, theme: &Theme) -> AnyElement {
    let svg = gpui::svg()
        .path("icons/loader.svg")
        .flex_none()
        .size(px(14.))
        .text_color(theme.text_3);
    if theme.ui.reduce_motion {
        return svg.into_any_element();
    }
    svg.with_animation(
        id,
        Animation::new(Duration::from_millis(900)).repeat(),
        |svg, delta| {
            svg.with_transformation(Transformation::rotate(gpui::radians(
                delta * std::f32::consts::TAU,
            )))
        },
    )
    .into_any_element()
}

fn loading_state(theme: &Theme) -> AnyElement {
    div()
        .flex_1()
        .min_h_0()
        .flex()
        .items_center()
        .justify_center()
        .gap(px(8.))
        .child(spinner("viewer-loading", theme))
        .child(
            div()
                .text_size(theme.ui_px(12.5))
                .text_color(theme.text_3)
                .child(tr!("explorer.loading")),
        )
        .into_any_element()
}

fn centered_message(theme: &Theme, title: String, detail: Option<&str>) -> AnyElement {
    let mut column = div()
        .flex_1()
        .min_h_0()
        .flex()
        .flex_col()
        .items_center()
        .justify_center()
        .px(px(24.))
        .pb(px(24.))
        .child(
            div()
                .text_size(theme.ui_px(13.))
                .font_weight(FontWeight::MEDIUM)
                .text_color(theme.text)
                .child(title),
        );
    if let Some(detail) = detail {
        column = column.child(
            div()
                .mt(px(6.))
                .max_w(px(320.))
                .text_align(TextAlign::Center)
                .text_size(theme.ui_px(12.))
                .line_height(theme.ui_px(17.))
                .text_color(theme.text_3)
                .child(detail.to_string()),
        );
    }
    column.into_any_element()
}

/// `ListState::reset` is not `&mut` on the field through a closure borrow in
/// the load path; this keeps that call explicit and readable.
fn mono_font() -> Font {
    Font {
        family: theme::code_font_family(),
        features: FontFeatures::default(),
        fallbacks: None,
        weight: FontWeight::NORMAL,
        style: FontStyle::Normal,
    }
}

fn run(len: usize, color: Hsla, font: &Font) -> TextRun {
    TextRun {
        len,
        font: font.clone(),
        color,
        background_color: None,
        underline: None,
        strikethrough: None,
    }
}

/// Build syntax-colored text for one line. Paint-only: the tokens were
/// computed off-thread when the file loaded.
fn line_text(text: &str, tokens: &[Token], theme: Theme) -> StyledText {
    let font = mono_font();
    let base = theme.text_2;
    if tokens.is_empty() {
        return StyledText::new(text.to_string()).with_runs(vec![run(text.len(), base, &font)]);
    }
    let mut runs: Vec<TextRun> = Vec::new();
    let mut offset = 0usize;
    for token in tokens {
        let start = token.range.start.min(text.len());
        let end = token.range.end.min(text.len());
        if start > offset {
            runs.push(run(start - offset, base, &font));
        }
        if end > start {
            runs.push(run(end - start, theme.token_color(token.class), &font));
        }
        offset = offset.max(end);
    }
    if offset < text.len() {
        runs.push(run(text.len() - offset, base, &font));
    }
    if runs.is_empty() {
        runs.push(run(text.len(), base, &font));
    }
    StyledText::new(text.to_string()).with_runs(runs)
}

fn format_bytes(bytes: u64) -> String {
    const UNITS: [&str; 4] = ["B", "KB", "MB", "GB"];
    let mut value = bytes as f64;
    let mut unit = 0;
    while value >= 1024. && unit < UNITS.len() - 1 {
        value /= 1024.;
        unit += 1;
    }
    if unit == 0 {
        format!("{bytes} {}", UNITS[0])
    } else {
        format!("{value:.1} {}", UNITS[unit])
    }
}

impl Render for FileViewer {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        if !self.open {
            return div().into_any_element();
        }
        if self.focus_pending {
            self.focus_pending = false;
            window.focus(&self.focus);
        }
        let theme = *theme::get(cx);
        div()
            .id("file-viewer")
            .key_context("Files")
            .track_focus(&self.focus)
            .size_full()
            .min_w_0()
            .min_h_0()
            .flex()
            .flex_col()
            .bg(theme.bg_main)
            .child(self.tab_strip(theme, cx))
            .child(self.toolbar(theme, cx))
            .child(self.body(theme, cx))
            .into_any_element()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join("orbit-explorer-viewer-tests");
        std::fs::create_dir_all(&dir).unwrap();
        dir.join(name)
    }

    #[test]
    fn loads_code_with_tokens_and_drops_the_trailing_blank_line() {
        let path = temp("sample.rs");
        std::fs::write(&path, "fn main() {\n    println!(\"hi\");\n}\n").unwrap();
        let content = load(&path, "sample.rs");
        assert!(matches!(content.mode, Mode::Code { lang: Lang::Rust }));
        assert_eq!(content.lines.len(), 3);
        assert!(content.tokens.is_some());
        assert!(content.error.is_none());
    }

    #[test]
    fn detects_markdown() {
        let path = temp("notes.md");
        std::fs::write(&path, "# Title\n\nbody\n").unwrap();
        let content = load(&path, "notes.md");
        assert_eq!(content.mode, Mode::Markdown);
        assert!(content.tokens.is_none());
        assert_eq!(content.lines[0], "# Title");
    }

    #[test]
    fn detects_binary_from_a_nul_byte() {
        let path = temp("blob.bin");
        std::fs::write(&path, [0u8, 1, 2, 3, 255]).unwrap();
        let content = load(&path, "blob.bin");
        assert_eq!(content.mode, Mode::Binary);
        assert!(content.lines.is_empty());
    }

    #[test]
    fn oversized_files_are_not_read() {
        let path = temp("big.txt");
        let file = std::fs::File::create(&path).unwrap();
        file.set_len(MAX_FILE_BYTES + 1).unwrap();
        drop(file);
        let content = load(&path, "big.txt");
        assert_eq!(content.mode, Mode::TooLarge);
        assert!(content.lines.is_empty());
        assert_eq!(content.bytes, MAX_FILE_BYTES + 1);
    }

    #[test]
    fn invalid_utf8_is_decoded_lossily_not_rejected() {
        let path = temp("latin1.txt");
        std::fs::write(&path, [b'c', b'a', b'f', 0xE9, b'\n']).unwrap();
        let content = load(&path, "latin1.txt");
        assert_eq!(content.mode, Mode::Text);
        assert!(content.lines[0].starts_with("caf"));
    }

    #[test]
    fn directories_report_an_error_instead_of_panicking() {
        let dir = std::env::temp_dir().join("orbit-explorer-viewer-tests/dir");
        std::fs::create_dir_all(&dir).unwrap();
        let content = load(&dir, "dir");
        assert!(content.error.is_some());
    }

    #[test]
    fn image_extensions_are_recognized() {
        assert!(is_image("a.png"));
        assert!(is_image("dir/B.JPEG"));
        assert!(!is_image("a.rs"));
        assert!(is_markdown("README.MD"));
    }

    /// A long run of open files must scroll horizontally instead of clipping
    /// off the right edge (the reported overflow). The strip keeps the last
    /// tabs reachable by giving the row a positive horizontal scroll range.
    #[gpui::test]
    fn tab_strip_scrolls_when_tabs_overflow(cx: &mut gpui::TestAppContext) {
        use gpui::{point, size};

        let cx = cx.add_empty_window();
        cx.update(|_, app| crate::theme::init(app));
        let viewer = cx.new(|cx| FileViewer::new(Rc::new(|_| {}), cx));
        viewer.update(cx, |viewer, cx| {
            for index in 0..12 {
                viewer.show(
                    PathBuf::from(format!("/tmp/orbit-viewer-tab-{index}.rs")),
                    format!("file-{index}.rs"),
                    cx,
                );
            }
        });
        let _ = cx.draw(point(px(0.), px(0.)), size(px(420.), px(600.)), |_, _| {
            viewer.clone()
        });
        let max = cx.update(|_, app| viewer.read(app).tab_scroll.max_offset().width);
        assert!(
            max > px(0.),
            "the strip must scroll when 12 tabs overflow a 420px window, max offset {max:?}"
        );
    }
}
