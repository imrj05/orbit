use super::*;
use crate::commands::CommandId;
use crate::theme::tokens::popover;

impl OrbitApp {
    /// Rows for the new-task page's folder selector, in priority order:
    /// this repository's locations (the main checkout, then each linked
    /// worktree), then distinct session folders newest-first (`load_sessions`
    /// pre-sorts), with the current workspace pinned to the top when it is not
    /// already one of them. Every row is a place a task can start in, so the
    /// same list serves the Worktrees page's vocabulary.
    pub(super) fn recent_workspaces(&self) -> Vec<WorkspaceEntry> {
        let current = self
            .current_workspace
            .clone()
            .or_else(|| std::env::current_dir().ok());
        let mut entries: Vec<WorkspaceEntry> = Vec::new();
        let push = |entry: WorkspaceEntry, entries: &mut Vec<WorkspaceEntry>| {
            if entries.iter().any(|e| e.path == entry.path) {
                return;
            }
            entries.push(entry);
        };

        // The repository first: "Local" and each worktree, exactly as the
        // status bar's picker orders them.
        if let Some(root) = self.work_in_root() {
            for row in
                crate::working_folder::location_rows(&self.worktrees, &root, current.as_deref())
            {
                let mark = match &row.folder.location {
                    crate::working_folder::Location::Local => EntryMark::Local,
                    crate::working_folder::Location::Worktree { .. } => EntryMark::Worktree,
                    _ => EntryMark::Folder,
                };
                push(
                    WorkspaceEntry {
                        name: row.label,
                        path: row.folder.path,
                        note: row.detail,
                        mark,
                    },
                    &mut entries,
                );
            }
        }

        if let Some(cwd) = &current {
            push(
                WorkspaceEntry {
                    name: sessions::workspace_label(cwd),
                    path: cwd.clone(),
                    note: None,
                    mark: EntryMark::Folder,
                },
                &mut entries,
            );
        }
        for session in &self.sessions {
            if entries.len() >= crate::workspace_picker::MAX_RECENTS {
                break;
            }
            push(
                WorkspaceEntry {
                    name: sessions::workspace_label(&session.cwd),
                    path: session.cwd.clone(),
                    note: Some(sessions::relative_time(session.modified)),
                    mark: EntryMark::Folder,
                },
                &mut entries,
            );
        }
        entries
    }

    /// Worktree actions require a repository; cloning does not.
    pub(super) fn folder_actions(&self) -> Vec<PickerAction> {
        let mut actions = if self.work_in_root().is_some() {
            vec![
                PickerAction {
                    id: "new-worktree",
                    icon: "icons/plus.svg",
                    label: tr!("working_folder.new"),
                    hint: Some(tr!("working_folder.new_hint")),
                },
                PickerAction {
                    id: "existing-worktree",
                    icon: "icons/branch.svg",
                    label: tr!("working_folder.existing"),
                    hint: Some(tr!("working_folder.existing_hint")),
                },
                PickerAction {
                    id: "remote-branch",
                    icon: "icons/cloud.svg",
                    label: tr!("working_folder.remote"),
                    hint: Some(tr!("working_folder.remote_hint")),
                },
            ]
        } else {
            Vec::new()
        };
        actions.push(PickerAction {
            id: "clone-repository",
            icon: "icons/cloud.svg",
            label: tr!("working_folder.clone_title"),
            hint: Some(tr!("working_folder.clone_hint")),
        });
        actions
    }

    /// Toggle the workspace picker under the new-task page's folder field.
    /// Mutually exclusive with the other popovers; Escape/outside-down
    /// dismiss returns focus to the composer.
    pub(super) fn toggle_workspace_picker(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        const GESTURE: Duration = Duration::from_millis(200);
        if let Some(dismissed) = self.menu_dismissed_at.take() {
            if dismissed.elapsed() < GESTURE {
                return;
            }
        }
        if self.workspace_picker.take().is_some() {
            self.input.read(cx).focus(window);
            cx.notify();
            return;
        }
        if self.model_selector.is_some() {
            self.close_model_selector(window, cx);
        }
        self.branch_picker = None;
        self.session_menu = None;

        let entries = self.recent_workspaces();
        let actions = self.folder_actions();
        let current = self
            .current_workspace
            .clone()
            .or_else(|| std::env::current_dir().ok());
        // Match the field's width basis exactly (main area = window minus the
        // sidebar) so the popover lands flush under the card at any size.
        let main_width = f32::from(window.viewport_size().width)
            - if self.sidebar_visible && !self.settings_open {
                self.sidebar_width.into()
            } else {
                0.
            };
        let width = crate::workspace_picker::width_for_window(main_width);

        let this = cx.weak_entity();
        let on_pick = Box::new(move |folder: PathBuf, window: &mut Window, cx: &mut App| {
            this.update(cx, |app, cx| {
                app.workspace_picker = None;
                app.start_task_in_folder(folder, window, cx);
            })
            .ok();
        }) as Box<dyn Fn(PathBuf, &mut Window, &mut App)>;
        let this = cx.weak_entity();
        let on_browse = Box::new(move |window: &mut Window, cx: &mut App| {
            this.update(cx, |app, cx| {
                app.workspace_picker = None;
                cx.notify();
                app.browse_for_folder(window, cx);
            })
            .ok();
        }) as Box<dyn Fn(&mut Window, &mut App)>;
        // Verbs the picker offers under the folders. Each one closes the
        // picker first: they open a dialog or a second surface, and a popover
        // left behind a modal is the wrong layer.
        let this = cx.weak_entity();
        let action_ids: Vec<&'static str> = actions.iter().map(|action| action.id).collect();
        let on_action = Box::new(move |ix: usize, window: &mut Window, cx: &mut App| {
            let Some(id) = action_ids.get(ix).copied() else {
                return;
            };
            this.update(cx, |app, cx| {
                app.workspace_picker = None;
                match id {
                    "new-worktree" => app.open_new_worktree_from_work_in(window, cx),
                    "existing-worktree" => app.browse_for_existing_worktree(window, cx),
                    "clone-repository" => app.open_clone_dialog(window, cx),
                    // The create dialog's existing-branch list carries the
                    // remote refs, and the manager turns one into a local
                    // tracking branch instead of a detached worktree.
                    _ => app.open_new_worktree_from_remote_branch(window, cx),
                }
            })
            .ok();
        }) as Box<dyn Fn(usize, &mut Window, &mut App)>;
        let this = cx.weak_entity();
        let on_dismiss = Box::new(move |by_mouse: bool, window: &mut Window, cx: &mut App| {
            this.update(cx, |app, cx| {
                if by_mouse {
                    app.menu_dismissed_at = Some(Instant::now());
                }
                app.workspace_picker = None;
                app.input.read(cx).focus(window);
                cx.notify();
            })
            .ok();
        }) as Box<dyn Fn(bool, &mut Window, &mut App)>;

        let picker = cx.new(|cx| {
            WorkspacePicker::new(
                entries, actions, current, width, on_pick, on_browse, on_action, on_dismiss, cx,
            )
        });
        window.focus(&picker.read(cx).focus_handle(cx));
        self.workspace_picker = Some(picker);
        cx.notify();
    }

    /// Start a new task rooted at `folder`: spawn a fresh pi process with
    /// that working directory (the agent reads, edits, and runs commands
    /// there), reset the transcript/state, and focus the composer. A run in
    /// flight is parked instead of dropped — its process keeps going in the
    /// background — so starting a task never aborts the current one; an idle
    /// session is torn down (its transcript is already on disk).
    pub(super) fn start_task_in_folder(
        &mut self,
        folder: PathBuf,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.start_task_in_folder_quiet(folder, cx);
        // Ready to type: the empty state is gone, so put the caret in the
        // composer. Callers with no window (the Worktrees page's Open, the
        // status bar's location rows) use the quiet form.
        self.input.read(cx).focus(window);
        cx.notify();
    }

    /// [`Self::start_task_in_folder`] without taking a window: the workspace
    /// move, the spawn, and the reset, but no focus change. The caller
    /// notifies.
    pub(super) fn start_task_in_folder_quiet(&mut self, folder: PathBuf, cx: &mut Context<Self>) {
        if self.rewind_transition_pending(cx) {
            return;
        }
        if self.is_running() {
            // Blocking native requests stay pending with the parked session.
            self.cancel_custom_surfaces(cx);
            self.park_active_session();
        } else {
            self.drop_client();
        }
        self.execution = SessionExecution::default();
        self.transcript.clear();
        self.current_title = None;
        self.reset_session_name(cx);
        // The parked run's busy state lives with the parked session; this
        // view starts idle.
        self.busy = false;
        // Picking a folder to work in adds it to Orbit's own sidebar list.
        self.add_workspace(folder.clone(), cx);
        self.set_current_workspace(folder.clone(), cx);
        self.current_session_path = None;
        self.added = 0;
        self.removed = 0;
        self.context = None;
        self.session_usage = None;
        self.reset_turns();
        self.reset_queue();
        match self.extensions.spawn(&folder, true, &self.mcp.secret_env()) {
            Ok(client) => {
                self.adopt_client(client);
                self.mcp_stamp = self.mcp.fingerprint_for(self.current_workspace.as_deref());
                self.send(CommandBody::GetState, "get_state");
                self.refresh_catalogs();
                // Capability probes queue after the state request.
                self.probe_auth();
                self.set_status(tr!("pickers.new_task_started"));
            }
            Err(err) => {
                let message = tr!("runtime.pi_spawn_failed", error = err);
                self.client = None;
                self.runtime.error = Some(message.clone());
                self.set_status(message);
            }
        }
    }

    /// Toggle one of the two composer dropdowns (model / thinking). Opening
    /// one closes the other; clicking the open chip closes it. Re-checks the
    /// catalog on open so the list always reflects the live pi session.
    pub(super) fn toggle_picker(
        &mut self,
        kind: PickerKind,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.picker_is_open(kind) {
            self.close_model_selector(window, cx);
            return;
        }
        if self.model_selector.is_some() {
            self.close_model_selector(window, cx);
        }
        self.dismiss_work_in_menu();
        self.open_picker(kind, window, cx);
    }

    pub(super) fn picker_is_open(&self, kind: PickerKind) -> bool {
        matches!(&self.model_selector, Some((open_kind, _)) if *open_kind == kind)
    }

    pub(super) fn open_picker(
        &mut self,
        kind: PickerKind,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        // Model and thinking are locked while a run is in flight.
        if self.is_running() {
            return;
        }
        self.refresh_catalogs();
        self.send(CommandBody::GetState, "get_state");
        // Mutually exclusive with the composer's add menu and the new-task
        // page's workspace picker.
        self.add_menu_open = false;
        self.access_menu_open = false;
        self.workspace_picker = None;

        // The popup talks back exclusively through these callbacks; it never
        // borrows app state.
        let this = cx.weak_entity();
        let on_select_model = Box::new(
            move |id: &str, provider: &str, window: &mut Window, cx: &mut App| {
                // A user pick is what "recently used" means — the default push
                // for a fresh session is not recorded.
                crate::recent_models::record(provider, id);
                this.update(cx, |app, cx| {
                    app.set_model(id.to_string(), provider.to_string(), cx);
                    app.close_model_selector(window, cx);
                })
                .ok();
            },
        ) as Box<dyn Fn(&str, &str, &mut Window, &mut App)>;
        let this = cx.weak_entity();
        let on_select_level = Box::new(move |level: &str, window: &mut Window, cx: &mut App| {
            this.update(cx, |app, cx| {
                app.set_thinking_level(level.to_string(), cx);
                app.close_model_selector(window, cx);
            })
            .ok();
        }) as Box<dyn Fn(&str, &mut Window, &mut App)>;
        let this = cx.weak_entity();
        let on_dismiss = Box::new(move |by_mouse: bool, window: &mut Window, cx: &mut App| {
            this.update(cx, |app, cx| {
                // Only mouse dismissals arm the chip's click-through guard.
                if by_mouse {
                    app.menu_dismissed_at = Some(Instant::now());
                }
                app.close_model_selector(window, cx);
            })
            .ok();
        }) as Box<dyn Fn(bool, &mut Window, &mut App)>;

        let selector = cx.new(|cx| {
            ModelSelector::new(
                kind,
                self.available_models.clone(),
                self.available_thinking_levels.clone(),
                self.model_label.clone(),
                self.model_id.clone(),
                self.model_provider.clone(),
                self.thinking_label.clone(),
                on_select_model,
                on_select_level,
                on_dismiss,
                cx,
            )
        });
        // Focus the popup's filter input so typing filters immediately.
        window.focus(&selector.read(cx).focus_handle(cx));
        self.model_selector = Some((kind, selector));
        cx.notify();
    }

    /// Drop the popup (if open) and put focus back on the composer.
    pub(super) fn close_model_selector(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.model_selector.take().is_some() {
            self.input.read(cx).focus(window);
            cx.notify();
        }
    }

    pub(super) fn on_toggle_command_palette(
        &mut self,
        _: &crate::ToggleCommandPalette,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        // Keyboard path: no gesture guard. A dismissed popover must not be able
        // to swallow the next ⌘P/⌘K — the guard exists for a single pointer
        // gesture that both closes and re-opens, which cannot happen here.
        self.toggle_command_palette_with(false, window, cx);
    }

    /// Exercise the pointer gesture guard in popup tests.
    #[cfg(test)]
    pub(super) fn toggle_command_palette(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.toggle_command_palette_with(true, window, cx);
    }

    /// Hand focus back to whatever opened the palette, or to the composer when
    /// that control is gone. Every close path goes through here.
    pub(super) fn restore_palette_focus(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        match self.palette_return_focus.take().and_then(|h| h.upgrade()) {
            Some(handle) => window.focus(&handle),
            None => self.input.read(cx).focus(window),
        }
    }

    fn toggle_command_palette_with(
        &mut self,
        by_gesture: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        // A dismissal from the same click's mouse-down must not immediately
        // re-open — the sidebar Search row opens on mouse-up, and the scrim
        // dismisses on mouse-down.
        const GESTURE: Duration = Duration::from_millis(200);
        if by_gesture {
            if let Some(dismissed) = self.menu_dismissed_at.take() {
                if dismissed.elapsed() < GESTURE {
                    return;
                }
            }
        }
        if self.command_palette.take().is_some() {
            self.restore_palette_focus(window, cx);
            cx.notify();
            return;
        }
        if self.model_selector.is_some() {
            self.close_model_selector(window, cx);
        }
        self.branch_picker = None;
        self.workspace_picker = None;
        self.session_menu = None;
        self.workspace_menu = None;

        // Model and thinking choices are fixed for the duration of a turn, so
        // the palette hides those two commands while a run is in flight.
        let running = self.is_running();
        let snapshot = PaletteSnapshot {
            sessions: self.sidebar_sessions(),
            active_path: self.current_session_path.clone(),
            active_pinned: self
                .current_session_path
                .as_ref()
                .is_some_and(|path| crate::pins::contains(path)),
            busy: running,
            session_id: self.session_id.clone(),
            sidebar_visible: self.sidebar_visible,
            side_panel_visible: self.sidepane.read(cx).is_open(),
            terminal_visible: self.terminal_panel.read(cx).is_open(),
            terminal_tabs: self.terminal_panel.read(cx).tab_count(),
            project_panel_visible: self.project_panel.read(cx).is_open(),
            can_choose_model: !self.available_models.is_empty() && !running,
            can_choose_thinking: self.can_choose_thinking() && !running,
            mcp_servers: self
                .mcp
                .servers()
                .iter()
                .map(|server| command_palette::PaletteMcpServer {
                    name: server.name.clone(),
                    enabled: server.def.enabled,
                })
                .collect(),
        };

        let this = cx.weak_entity();
        let on_open = Box::new(
            move |session: SessionInfo, _window: &mut Window, cx: &mut App| {
                this.update(cx, |app, cx| {
                    app.command_palette = None;
                    app.on_open_session(session, cx);
                })
                .ok();
            },
        ) as Box<dyn Fn(SessionInfo, &mut Window, &mut App)>;
        let this = cx.weak_entity();
        let on_command = Box::new(
            move |command: PaletteCommand, window: &mut Window, cx: &mut App| {
                this.update(cx, |app, cx| {
                    app.command_palette = None;
                    app.restore_palette_focus(window, cx);
                    app.run_palette_command(command, window, cx);
                })
                .ok();
            },
        ) as Box<dyn Fn(PaletteCommand, &mut Window, &mut App)>;
        let this = cx.weak_entity();
        let on_dismiss = Box::new(move |by_mouse: bool, window: &mut Window, cx: &mut App| {
            this.update(cx, |app, cx| {
                if by_mouse {
                    app.menu_dismissed_at = Some(Instant::now());
                }
                app.command_palette = None;
                app.restore_palette_focus(window, cx);
                cx.notify();
            })
            .ok();
        }) as Box<dyn Fn(bool, &mut Window, &mut App)>;

        let palette =
            cx.new(|cx| CommandPalette::new(snapshot, on_open, on_command, on_dismiss, cx));
        // Remember who opened it (the focused control, or the composer when the
        // palette is opened from a menu that took focus), then focus the filter.
        self.palette_return_focus = Some(
            window
                .focused(cx)
                .unwrap_or_else(|| self.input.read(cx).focus_handle(cx))
                .downgrade(),
        );
        window.focus(&palette.read(cx).focus_handle(cx));
        self.command_palette = Some(palette);
        cx.notify();
    }

    /// Execute a command chosen in the palette (the palette is already
    /// closed; focus returns to the composer unless the command opens
    /// another surface).
    pub(super) fn run_palette_command(
        &mut self,
        command: PaletteCommand,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match command {
            PaletteCommand::Run(id) => self.run_command(id, window, cx),
            PaletteCommand::OpenSettings(section) => {
                self.settings_open = true;
                self.set_settings_section(section, cx);
                self.sync_terminal_active(cx);
            }
            PaletteCommand::Mcp { name, action } => match action {
                crate::command_palette::McpPaletteAction::Toggle => {
                    self.mcp_toggle_server(name, cx)
                }
                crate::command_palette::McpPaletteAction::Reconnect => {
                    self.mcp_reconnect_server(name, cx)
                }
                crate::command_palette::McpPaletteAction::Test => self.mcp_test_server(name, cx),
            },
        }
    }

    /// Run one registry command by id. The palette is the first caller; the
    /// keymap dispatches the same commands as typed actions, so both paths
    /// meet in one place where the UI can also call them directly.
    pub(super) fn run_command(
        &mut self,
        id: CommandId,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match id {
            CommandId::NewSession => self.on_new_session(&crate::NewSession, window, cx),
            CommandId::RefreshSessions => self.on_refresh(&crate::RefreshSessions, window, cx),
            CommandId::FocusComposer => {
                self.input.read(cx).focus(window);
            }
            CommandId::FocusSessions => {
                self.on_focus_sessions(&crate::FocusSessions, window, cx);
            }
            // A numbered slot needs its payload action; the palette never
            // lists it, so there is nothing to run here.
            CommandId::OpenSessionSlot => {}
            CommandId::NextSession => self.cycle_session(1, cx),
            CommandId::PrevSession => self.cycle_session(-1, cx),
            CommandId::RenameSession => self.begin_session_rename(window, cx),
            CommandId::PinSession => {
                if let Some(path) = self.current_session_path.clone() {
                    crate::pins::toggle(&path);
                    self.toast_success(if crate::pins::contains(&path) {
                        tr!("sidebar.pin_session")
                    } else {
                        tr!("sidebar.unpin_session")
                    });
                    cx.notify();
                }
            }
            CommandId::CloneSession => self.clone_session(cx),
            CommandId::RewindConversation => self.open_rewind_picker(window, cx),
            CommandId::ExportVisibleTranscript => self.export_markdown(false, cx),
            CommandId::ExportVisibleTranscriptPdf => self.export_visible_pdf(cx),
            CommandId::ExportLastResponsePdf => self.export_last_response_pdf(cx),
            CommandId::CopySessionId => {
                if let Some(id) = self.session_id.clone() {
                    cx.write_to_clipboard(ClipboardItem::new_string(id));
                    self.toast_success(tr!("pickers.session_id_copied"));
                    cx.notify();
                }
            }
            CommandId::DeleteSession => self.arm_active_session_delete(window, cx),
            CommandId::ChooseModel => self.toggle_picker(PickerKind::Model, window, cx),
            CommandId::ChooseThinking => self.toggle_picker(PickerKind::Thinking, window, cx),
            // The guard policy has no chord, so the palette is its keyboard
            // path; the composer chip is the pointer one. Opening the menu
            // moves focus to it, so ↑/↓/Enter/Escape take over from here.
            CommandId::ChooseAccess => self.toggle_access_menu(window, cx),
            // Same shape: the scope chip is the pointer path, the palette the
            // keyboard one.
            CommandId::ChooseWorkflowMode => self.toggle_workflow_menu(window, cx),
            CommandId::AbortRun => self.on_abort(&crate::AbortRun, window, cx),
            CommandId::ToggleCommandPalette => {
                self.on_toggle_command_palette(&crate::ToggleCommandPalette, window, cx)
            }
            CommandId::ToggleSearch => self.on_toggle_search(&crate::ToggleSearch, window, cx),
            CommandId::PrevTurn => self.on_prev_turn(&crate::PrevTurn, window, cx),
            CommandId::NextTurn => self.on_next_turn(&crate::NextTurn, window, cx),
            CommandId::CopyLastResponse => {
                self.on_copy_last_response(&crate::CopyLastResponse, window, cx)
            }
            CommandId::ToggleUsage => self.on_toggle_usage(&crate::ToggleUsage, window, cx),
            CommandId::ReviewChanges => {
                self.open_review(cx);
            }
            CommandId::OpenGit => {
                self.command_palette = None;
                self.open_git(cx);
            }
            CommandId::OpenWorktrees => {
                self.on_open_worktrees(&crate::OpenWorktrees, window, cx);
            }
            CommandId::FocusSessionSearch => {
                self.focus_session_search(window, cx);
            }
            CommandId::ToggleSidebar => {
                self.toggle_sidebar(window, cx);
                cx.notify();
            }
            CommandId::ToggleSidePanel => {
                // Opening Review replaces the Git page, which shares the window.
                if !self.sidepane.read(cx).is_open() {
                    self.close_git_for_review(cx);
                }
                self.sidepane.update(cx, |pane, cx| pane.toggle(cx));
            }
            CommandId::ToggleTerminal => {
                self.terminal_panel
                    .update(cx, |panel, cx| panel.toggle(window, cx));
            }
            CommandId::NewTerminalTab => {
                self.on_new_terminal_tab(&crate::NewTerminalTab, window, cx);
            }
            CommandId::CloseTerminalTab => {
                self.on_close_terminal_tab(&crate::CloseTerminalTab, window, cx);
            }
            CommandId::NextTerminalTab => {
                self.on_next_terminal_tab(&crate::NextTerminalTab, window, cx);
            }
            CommandId::PrevTerminalTab => {
                self.on_prev_terminal_tab(&crate::PrevTerminalTab, window, cx);
            }
            // Route through the app so opening the tree also closes Review;
            // the two docks are mutually exclusive.
            CommandId::ToggleProjectPanel => self.toggle_project_panel(cx),
            CommandId::GitTabChanges => self.on_git_tab(0, window, cx),
            CommandId::GitTabHistory => self.on_git_tab(1, window, cx),
            CommandId::GitTabGraph => self.on_git_tab(2, window, cx),
            CommandId::GitTabStashes => self.on_git_tab(3, window, cx),
            CommandId::GitTabIssues => self.on_git_tab(4, window, cx),
            CommandId::GitTabPulls => self.on_git_tab(5, window, cx),
            CommandId::OpenSettings => {
                self.on_open_settings(&crate::OpenSettings, window, cx);
            }
            CommandId::OpenMcpSettings => {
                self.command_palette = None;
                self.settings_open = true;
                self.set_settings_section(SettingsSection::Mcp, cx);
                self.sync_terminal_active(cx);
            }
            CommandId::AddMcpServer => {
                self.command_palette = None;
                self.settings_open = true;
                self.set_settings_section(SettingsSection::Mcp, cx);
                self.sync_terminal_active(cx);
                self.mcp_add(window, cx);
            }
            CommandId::RefreshMcpServers => {
                self.command_palette = None;
                self.mcp_refresh(cx);
            }
            CommandId::ReconnectMcpServers => {
                self.command_palette = None;
                self.mcp_reconnect_all(cx);
            }
            CommandId::OpenShortcutHelp => {
                self.settings_open = true;
                self.set_settings_section(SettingsSection::Shortcuts, cx);
                self.sync_terminal_active(cx);
            }
            CommandId::CheckForUpdates => {
                self.on_check_for_updates(&crate::CheckForUpdates, window, cx)
            }
            CommandId::Quit => cx.quit(),
            // Context-only commands are dispatched by their own focus nodes
            // (the review tree), never by the palette.
            CommandId::ReviewTreeNext
            | CommandId::ReviewTreePrev
            | CommandId::ReviewTreeToggle
            | CommandId::ReviewFileNext
            | CommandId::ReviewFilePrev
            | CommandId::ReviewHunkNext
            | CommandId::ReviewHunkPrev
            | CommandId::ReviewExpandAll
            | CommandId::ReviewCollapseAll
            | CommandId::ReviewClose => {}
        }
    }

    /// Toggle the Git branch picker from the status-bar branch chip.
    pub(super) fn toggle_branch_picker(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.branch_operation_pending {
            return;
        }
        const GESTURE: Duration = Duration::from_millis(200);
        if let Some(dismissed) = self.menu_dismissed_at.take() {
            if dismissed.elapsed() < GESTURE {
                return;
            }
        }
        if self.branch_picker.take().is_some() {
            self.input.read(cx).focus(window);
            cx.notify();
            return;
        }
        if self.model_selector.is_some() {
            self.close_model_selector(window, cx);
        }
        self.workspace_picker = None;
        self.session_menu = None;
        self.dismiss_work_in_menu();

        let cwd = match self
            .current_workspace
            .clone()
            .or_else(|| std::env::current_dir().ok())
        {
            Some(cwd) => cwd,
            None => return,
        };
        if !crate::git::is_repo(&cwd) {
            self.toast_warning(tr!("git.not_a_repository"));
            cx.notify();
            return;
        }
        let current = crate::git::current_branch(&cwd).unwrap_or_else(|| "HEAD".into());
        let branches = crate::git::list_branch_entries(&cwd).unwrap_or_else(|err| {
            self.toast_error(tr!("pickers.branch_list_failed", error = err));
            vec![crate::git::BranchEntry {
                name: current.clone(),
                commit_date: None,
            }]
        });
        let workspace_label = sessions::workspace_label(&cwd);

        let this = cx.weak_entity();
        let on_checkout = Box::new(move |branch: String, _window: &mut Window, cx: &mut App| {
            this.update(cx, |app, cx| {
                app.checkout_branch(branch, cx);
            })
            .ok();
        }) as Box<dyn Fn(String, &mut Window, &mut App)>;
        let this = cx.weak_entity();
        let on_create = Box::new(move |name: String, _window: &mut Window, cx: &mut App| {
            this.update(cx, |app, cx| {
                app.create_branch(name, cx);
            })
            .ok();
        }) as Box<dyn Fn(String, &mut Window, &mut App)>;
        let this = cx.weak_entity();
        let on_dismiss = Box::new(move |by_mouse: bool, _window: &mut Window, cx: &mut App| {
            this.update(cx, |app, cx| {
                if by_mouse {
                    app.menu_dismissed_at = Some(Instant::now());
                }
                app.branch_picker = None;
                cx.notify();
            })
            .ok();
        }) as Box<dyn Fn(bool, &mut Window, &mut App)>;

        let picker = cx.new(|cx| {
            BranchPicker::new(
                workspace_label,
                branches,
                current,
                on_checkout,
                on_create,
                on_dismiss,
                cx,
            )
        });
        window.focus(&picker.read(cx).focus_handle(cx));
        self.branch_picker = Some(picker);
        cx.notify();
    }

    pub(super) fn checkout_branch(&mut self, branch: String, cx: &mut Context<Self>) {
        let Some(cwd) = self
            .current_workspace
            .clone()
            .or_else(|| std::env::current_dir().ok())
        else {
            return;
        };
        self.branch_operation_pending = true;
        self.branch_picker = None;
        cx.notify();

        let label = branch.clone();
        cx.spawn(async move |this, cx| {
            let result = cx
                .background_executor()
                .spawn(async move { crate::git::checkout_branch(&cwd, &branch) })
                .await;
            let _ = this.update(cx, |app, cx| {
                app.branch_operation_pending = false;
                match result {
                    Ok(()) => app.toast_success(tr!("pickers.switched_to", label = label)),
                    Err(err) => app.toast_error(tr!("pickers.branch_switch_failed", error = err)),
                }
                app.refresh_branch_status(cx);
                cx.notify();
            });
        })
        .detach();
    }

    pub(super) fn create_branch(&mut self, name: String, cx: &mut Context<Self>) {
        let Some(cwd) = self
            .current_workspace
            .clone()
            .or_else(|| std::env::current_dir().ok())
        else {
            return;
        };
        self.branch_operation_pending = true;
        self.branch_picker = None;
        cx.notify();

        let label = name.clone();
        cx.spawn(async move |this, cx| {
            let result = cx
                .background_executor()
                .spawn(async move { crate::git::create_and_checkout_branch(&cwd, &name) })
                .await;
            let _ = this.update(cx, |app, cx| {
                app.branch_operation_pending = false;
                match result {
                    Ok(()) => app.toast_success(tr!("pickers.created_and_switched", label = label)),
                    // The key's placeholder is `%{err}` — binding `error`
                    // left the literal `%{err}` in the toast.
                    Err(err) => app.toast_error(tr!("pickers.branch_create_failed", err = err)),
                }
                app.refresh_branch_status(cx);
                cx.notify();
            });
        })
        .detach();
    }

    /// Popover anchored below the new-task page's workspace field.
    pub(super) fn workspace_picker_popup(&self) -> Option<AnyElement> {
        self.workspace_picker.clone().map(|picker| {
            div()
                .absolute()
                .bottom_0()
                .left_0()
                .size(px(0.))
                .child(anchored_popover(
                    Corner::TopLeft,
                    point(px(0.), popover::MENU_OFFSET),
                    picker,
                ))
                .into_any_element()
        })
    }

    /// Popover anchored above the status-bar branch chip.
    pub(super) fn branch_picker_popup(&self) -> Option<AnyElement> {
        self.branch_picker.clone().map(|picker| {
            div()
                .absolute()
                .bottom_0()
                .left_0()
                .size(px(0.))
                .child(anchored_popover(
                    Corner::BottomLeft,
                    point(px(0.), -popover::MENU_OFFSET),
                    picker,
                ))
                .into_any_element()
        })
    }

    /// Push the latest catalog/current-selection snapshot into the open
    /// popup (no-op while it is closed).
    pub(super) fn sync_model_selector(&mut self, cx: &mut Context<Self>) {
        if let Some((_, selector)) = &self.model_selector {
            let models = self.available_models.clone();
            let levels = self.available_thinking_levels.clone();
            let model = self.model_label.clone();
            let model_id = self.model_id.clone();
            let provider = self.model_provider.clone();
            let level = self.thinking_label.clone();
            selector.update(cx, |selector, cx| {
                selector.set_catalog(models, levels, model, model_id, provider, level, cx)
            });
        }
    }
}
