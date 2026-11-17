//! Project-header drag & drop (issue #143): the paint half. The reorder
//! algebra and the displayed-order pin live in `sidebar_sort_tests`; here the
//! question is only *where the insertion line lands*, because that is what
//! tells the user which group a release would move.

use super::sidebar::*;
use super::*;

use std::time::SystemTime;

/// Draws one workspace header with a project drag in flight, so the drop
/// affordance can be inspected without mounting the whole sidebar.
struct HeaderProbe {
    app: Entity<OrbitApp>,
    label: &'static str,
    drag: ProjectDragState,
    theme: Theme,
}

impl gpui::Render for HeaderProbe {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        let rows = Rc::new(vec![SideRow::Workspace {
            label: self.label.to_string(),
            count: 0,
            collapsed: false,
            cwd: PathBuf::from(format!("/drag-test/{}", self.label)),
        }]);
        render_side_row(
            &rows,
            &Rc::default(),
            None,
            0,
            &self.app,
            false,
            &Rc::default(),
            &Rc::default(),
            &Rc::default(),
            false,
            false,
            false,
            false,
            &Rc::default(),
            &Rc::default(),
            &Rc::default(),
            &Rc::default(),
            &Rc::default(),
            &Rc::default(),
            &Rc::default(),
            None,
            None,
            &SidebarDragPaint {
                project: Some(&self.drag),
                session: None,
                interactive: true,
                sessions: false,
            },
            false,
            None,
            false,
            self.theme,
        )
    }
}

/// The insertion line paints on the header a release would use — and on no
/// other header, so the affordance never claims a drop that would not happen.
#[gpui::test]
fn the_insertion_line_marks_only_the_target_header(cx: &mut gpui::TestAppContext) {
    cx.update(|cx| cx.set_global(Theme::for_id(crate::theme::ThemeId::Orbit)));
    let cx = cx.add_empty_window();
    let app = cx.update(|_, cx| cx.new(OrbitApp::new));
    let theme = Theme::for_id(crate::theme::ThemeId::Orbit);
    let drag = ProjectDragState {
        dragging: "alpha".into(),
        target: Some(ProjectDropTarget {
            label: "beta".into(),
            after: true,
        }),
    };

    let mut draw = |label: &'static str| {
        let probe = cx.update(|_, cx| {
            cx.new(|_| HeaderProbe {
                app: app.clone(),
                label,
                drag: drag.clone(),
                theme,
            })
        });
        cx.draw(
            point(px(0.), px(0.)),
            gpui::size(px(260.), px(80.)),
            |_, _| probe.clone(),
        );
        cx.debug_bounds("sidebar-project-drop-line")
    };

    let line = draw("beta").expect("the target header paints the insertion line");
    assert!(
        line.size.width > px(200.),
        "the line spans the header pill: {line:?}"
    );
    assert!(
        line.origin.x >= px(0.) && line.origin.x + line.size.width <= px(260.),
        "the line stays inside the sidebar column: {line:?}"
    );
    assert!(
        draw("gamma").is_none(),
        "a header that would not take the drop paints no line"
    );
}

/// Draws one session row with a session drag in flight, to check where the
/// insertion line lands.
struct SessionProbe {
    app: Entity<OrbitApp>,
    session: SessionInfo,
    drag: SessionDragState,
    theme: Theme,
}

impl gpui::Render for SessionProbe {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        let rows = Rc::new(vec![SideRow::Session(0)]);
        let sessions = Rc::new(vec![self.session.clone()]);
        render_side_row(
            &rows,
            &sessions,
            None,
            0,
            &self.app,
            false,
            &Rc::default(),
            &Rc::default(),
            &Rc::default(),
            false,
            false,
            false,
            false,
            &Rc::default(),
            &Rc::default(),
            &Rc::default(),
            &Rc::default(),
            &Rc::default(),
            &Rc::default(),
            &Rc::default(),
            None,
            None,
            &SidebarDragPaint {
                project: None,
                session: Some(&self.drag),
                interactive: true,
                sessions: true,
            },
            false,
            None,
            false,
            self.theme,
        )
    }
}

/// A session drag paints the insertion line on the row it would land next to,
/// and nowhere else.
#[gpui::test]
fn the_session_insertion_line_marks_only_the_target_row(cx: &mut gpui::TestAppContext) {
    cx.update(|cx| cx.set_global(Theme::for_id(crate::theme::ThemeId::Orbit)));
    let cx = cx.add_empty_window();
    let app = cx.update(|_, cx| cx.new(OrbitApp::new));
    let theme = Theme::for_id(crate::theme::ThemeId::Orbit);
    let session = SessionInfo {
        path: PathBuf::from("/drag-test/a1.jsonl"),
        id: "a1".into(),
        cwd: PathBuf::from("/drag-test/project"),
        title: "Rearrange me".into(),
        first_message: String::new(),
        modified: SystemTime::UNIX_EPOCH,
    };

    let mut draw = |target: Option<PathBuf>| {
        let drag = SessionDragState {
            dragging: PathBuf::from("/drag-test/dragged.jsonl"),
            target: target.map(|path| SessionDropTarget { path, after: false }),
        };
        let probe = cx.update(|_, cx| {
            cx.new(|_| SessionProbe {
                app: app.clone(),
                session: session.clone(),
                drag,
                theme,
            })
        });
        cx.draw(
            point(px(0.), px(0.)),
            gpui::size(px(260.), px(60.)),
            |_, _| probe.clone(),
        );
        cx.debug_bounds("sidebar-session-drop-line")
    };

    let line = draw(Some(session.path.clone())).expect("the landing row paints the line");
    assert!(
        line.size.width > px(200.),
        "the line spans the row: {line:?}"
    );
    assert!(
        draw(Some(PathBuf::from("/drag-test/other.jsonl"))).is_none(),
        "a row that is not the landing row paints no line"
    );
}
