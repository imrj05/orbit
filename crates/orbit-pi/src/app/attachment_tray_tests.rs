//! The composer's attachment tray: the chips it shows for images, files, and
//! pasted text, the count header and Clear action, and the one-budget rule
//! that keeps every attachment kind from multiplying into a tray no one
//! planned for.

use super::*;
use crate::theme::{Theme, ThemeId};

fn image_attachment(name: &str) -> Attachment {
    Attachment {
        name: name.into(),
        mime: "image/png".into(),
        data: String::new(),
        preview: None,
        original_len: 0,
        stored_len: 0,
        compressed: false,
    }
}

fn file(path: &str, size: u64) -> AttachedFile {
    AttachedFile {
        path: PathBuf::from(path),
        name: Path::new(path)
            .file_name()
            .map(|name| name.to_string_lossy().to_string())
            .unwrap_or_default(),
        size,
    }
}

fn app(cx: &mut gpui::TestAppContext) -> Entity<OrbitApp> {
    cx.update(|cx| {
        cx.set_global(Theme::for_id(ThemeId::Orbit));
        cx.new(OrbitApp::new)
    })
}

#[gpui::test]
fn pasted_file_paths_queue_real_image_and_file_attachments(cx: &mut gpui::TestAppContext) {
    let app = app(cx);
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let image = root.join("../../assets/icons/icon.png");
    let file = root.join("Cargo.toml");
    cx.update(|cx| {
        app.update(cx, |app, cx| {
            app.input
                .update(cx, |input, _| input.pasted_files.extend([image, file]));
            app.drain_pasted_files(cx);
            assert_eq!(app.attachments.len(), 1);
            assert_eq!(app.files.len(), 1);
            assert!(app.input.read(cx).pasted_files.is_empty());
        });
    });
}

/// The tray alone, so its shape can be read off the laid-out tree.
struct TrayProbe {
    app: Entity<OrbitApp>,
}

impl Render for TrayProbe {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        self.app.update(cx, |app, cx| {
            div().w(px(720.)).children(app.attachments_row(cx))
        })
    }
}

fn draw(cx: &mut gpui::VisualTestContext, app: &Entity<OrbitApp>) {
    let _ = cx.draw(
        point(px(0.), px(0.)),
        gpui::size(px(720.), px(400.)),
        |_, cx| cx.new(|_| TrayProbe { app: app.clone() }),
    );
}

/// A lone chip needs no label; once the tray holds more than one thing the
/// count appears, together with the one action that empties the tray without
/// touching the typed text.
#[gpui::test]
fn the_count_header_and_clear_appear_only_for_a_real_tray(cx: &mut gpui::TestAppContext) {
    cx.update(|cx| cx.set_global(Theme::for_id(ThemeId::Orbit)));
    let cx = cx.add_empty_window();
    let app = cx.update(|_, cx| cx.new(OrbitApp::new));

    cx.update(|_, cx| {
        app.update(cx, |app, _| {
            app.attachments.push(image_attachment("shot.png"));
        });
    });
    draw(cx, &app);
    assert!(
        cx.debug_bounds("attachments-row").is_some(),
        "one attachment still shows a chip"
    );
    assert!(
        cx.debug_bounds("attachments-clear").is_none(),
        "a single chip explains itself — no header, no Clear"
    );

    cx.update(|_, cx| {
        app.update(cx, |app, _| {
            app.files.push(file("/tmp/notes.md", 512));
        });
    });
    draw(cx, &app);
    assert!(
        cx.debug_bounds("attachments-clear").is_some(),
        "two attachments earn the count header and its Clear action"
    );

    cx.update(|_, cx| {
        app.update(cx, |app, cx| app.clear_attachments(cx));
    });
    cx.update(|_, cx| {
        app.update(cx, |app, cx| {
            assert!(
                app.attachments_row(cx).is_none(),
                "an emptied tray is not rendered at all"
            );
            assert!(!app.send_has_payload(cx), "nothing left to send");
        });
    });
}

/// Images, files, and pasted pills share one cap. The old rule counted images
/// alone, so a message could carry eight of each.
#[gpui::test]
fn every_attachment_kind_shares_one_budget(cx: &mut gpui::TestAppContext) {
    let app = app(cx);
    cx.update(|cx| {
        app.update(cx, |app, cx| {
            for ix in 0..MAX_ATTACHMENTS {
                app.attachments.push(image_attachment(&format!("{ix}.png")));
            }
            assert!(app.attachment_budget_exhausted());
            app.push_file(file("/tmp/notes.md", 12), cx);
            assert!(
                app.files.is_empty(),
                "a full tray turns a file away instead of queuing it beside the images"
            );

            app.attachments.clear();
            app.push_file(file("/tmp/notes.md", 12), cx);
            assert_eq!(app.files.len(), 1);
            assert_eq!(app.attachment_slots_used(), 1);

            // The same path twice is one attachment: the prompt needs the
            // mention once, and a duplicate chip only reads as a mistake.
            app.push_file(file("/tmp/notes.md", 12), cx);
            assert_eq!(app.files.len(), 1);
        });
    });
}

/// A path that names a regular file becomes a chip with its size; a folder is
/// not an attachment (the caller falls back to a plain path reference).
#[gpui::test]
fn a_file_attachment_records_the_name_and_size_it_will_reference(_cx: &mut gpui::TestAppContext) {
    let dir = std::env::temp_dir().join(format!("orbit-attach-{}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("temp dir");
    let path = dir.join("notes.md");
    std::fs::write(&path, b"hello").expect("write file");

    let file = AttachedFile::from_path(&path).expect("a regular file attaches");
    assert_eq!(file.name, "notes.md");
    assert_eq!(file.size, 5);
    assert!(
        AttachedFile::from_path(&dir).is_none(),
        "a folder is not a file attachment"
    );

    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn pasted_and_attached_images_have_the_same_preview() {
    let pixels = image::RgbaImage::from_pixel(2, 2, image::Rgba([12, 34, 56, 255]));
    let mut bytes = Vec::new();
    image::DynamicImage::ImageRgba8(pixels)
        .write_to(
            &mut std::io::Cursor::new(&mut bytes),
            image::ImageFormat::Png,
        )
        .expect("encode PNG");
    let image = gpui::Image::from_bytes(gpui::ImageFormat::Png, bytes.clone());
    let pasted = Attachment::from_image(&image, 0);
    let attached = Attachment::from_bytes("image.png".into(), &bytes, gpui::ImageFormat::Png);
    assert_eq!(
        pasted.preview.unwrap().bytes,
        attached.preview.unwrap().bytes
    );
    assert_eq!(pasted.data, attached.data);
}
