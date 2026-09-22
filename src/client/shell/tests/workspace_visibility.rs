use super::*;

fn create_state() -> ClientShellState {
    let mut projected = snapshot();
    let workspace = projected.workspaces[0].clone();
    let tab = projected.tabs[0].clone();
    let pane = projected.panes[0].clone();
    for number in 2..=3 {
        projected.workspaces.push(ClientShellWorkspace {
            workspace_id: format!("ws_{number}"),
            active_tab_id: format!("tab_{number}"),
            label: format!("project-{number}"),
            number,
            focused: false,
            ..workspace.clone()
        });
        projected.tabs.push(ClientShellTab {
            workspace_id: format!("ws_{number}"),
            tab_id: format!("tab_{number}"),
            focused: false,
            ..tab.clone()
        });
        projected.panes.push(ClientShellPane {
            workspace_id: format!("ws_{number}"),
            tab_id: format!("tab_{number}"),
            pane_id: format!("pane_{number}"),
            focused: false,
            ..pane.clone()
        });
    }
    projected.agents.push(ClientShellAgent {
        workspace_id: "ws_2".into(),
        tab_id: "tab_2".into(),
        pane_id: "pane_2".into(),
        name: Some("background agent".into()),
        display_agent: None,
        agent: Some("codex".into()),
        title: None,
        terminal_title: None,
        terminal_title_stripped: None,
        agent_status: AgentStatus::Working,
        state_change_seq: 1,
        state_labels: Vec::new(),
        tokens: Vec::new(),
        focused: false,
    });
    let mut state = ClientShellState::new(ClientShellConfig::from_config(&Config::default()));
    state.set_snapshot(Box::new(projected));
    state.set_pane_surface(surface());
    state
}

fn click(state: &mut ClientShellState, rect: Rect) -> ClientShellInput {
    state.handle_raw_events(vec![RawInputEvent::Mouse(MouseEvent {
        kind: MouseEventKind::Down(MouseButton::Left),
        column: rect.x,
        row: rect.y,
        modifiers: KeyModifiers::empty(),
    })])
}

#[test]
fn startup_preserves_hidden_default_focus_and_redirects_without_exposing_terminal() {
    let projected = create_state().snapshot.unwrap();
    let mut state = ClientShellState::new(ClientShellConfig::from_config(&Config::default()));
    state
        .hidden_workspaces
        .insert("local".into(), HashSet::from(["ws_1".into()]));
    state.set_snapshot(projected.clone());
    state.set_snapshot(projected.clone());
    state.set_pane_surface(surface_with_popup());
    assert!(state.hidden_workspaces["local"].contains("ws_1"));
    let frame = state.compose(106, 32).unwrap();
    let text = frame
        .cells
        .iter()
        .map(|cell| cell.symbol.as_str())
        .collect::<String>();
    assert!(text.contains("Choose a visible workspace"));
    assert!(!text.contains("LIVE"));
    assert!(!text.contains("popup-live"));
    assert!(state.hits.panes.is_empty());
    assert!(state.hits.popup.is_none());
    assert!(state.handle_input_bytes(b"x").requests.is_empty());
    let mut outcome = ClientShellInput::default();
    state.redirect_hidden_workspace_focus(&mut outcome);
    state.redirect_hidden_workspace_focus(&mut outcome);
    assert!(
        matches!(&outcome.actions[..], [ClientShellAction::Endpoint { request, .. }]
        if matches!(&request.method, crate::api::schema::Method::WorkspaceFocus(target) if target.workspace_id == "ws_2"))
    );
    let mut focused = *projected;
    focused.revision += 1;
    focused.focused_workspace_id = Some("ws_2".into());
    state.set_snapshot(Box::new(focused));
    assert!(state.hidden_workspaces["local"].contains("ws_1"));
    assert!(!state.is_focused_workspace_hidden());
}

#[test]
fn hidden_tree_restores_individually_without_closing_runtime_objects() {
    let mut state = create_state();
    let original = state.snapshot.clone();
    state.open_workspace_context_menu("ws_2".into(), 1, 1);
    let Some(ClientShellOverlay::ContextMenu(menu)) = state.overlay.as_ref() else {
        panic!("context menu")
    };
    let index = menu
        .items()
        .iter()
        .position(|item| item.action == ClientContextMenuAction::Hide)
        .unwrap();
    let mut outcome = ClientShellInput::default();
    state.activate_context_menu_item(index, &mut outcome);
    assert!(outcome.actions.is_empty());
    assert!(outcome.requests.is_empty());
    assert_eq!(state.snapshot, original);
    state.hide_workspace("ws_3".into(), &mut outcome);

    state.compose(106, 32).unwrap();
    assert_eq!(state.hits.workspaces.len(), 1);
    assert_eq!(state.hits.sidebar_tabs.len(), 1);
    assert!(state.hits.agents.is_empty());
    assert!(state.hits.hidden_workspaces.is_empty());
    let toggle = state.hits.hidden_workspaces_toggle;
    assert_ne!(toggle, Rect::default());
    assert!(click(&mut state, toggle).repaint);
    let frame = state.compose(106, 32).unwrap();
    let text = frame
        .cells
        .iter()
        .map(|cell| cell.symbol.as_str())
        .collect::<String>();
    assert!(text.contains("Hidden (2)"));
    assert!(text.contains("project-2 · Local"));
    assert_eq!(state.hits.hidden_workspaces.len(), 2);
    let row = state.hits.hidden_workspaces[0].0;
    let restored = click(&mut state, row);
    assert!(
        matches!(&restored.actions[..], [ClientShellAction::Endpoint { request, .. }]
        if matches!(&request.method, crate::api::schema::Method::WorkspaceFocus(target) if target.workspace_id == "ws_2"))
    );
    assert_eq!(
        state.hidden_workspaces["local"],
        HashSet::from(["ws_3".into()])
    );
    assert_eq!(state.snapshot, original);
}

#[test]
fn hide_active_waits_for_focus_and_protects_last_visible_workspace() {
    let mut state = create_state();
    let mut outcome = ClientShellInput::default();
    state.hide_workspace("ws_1".into(), &mut outcome);
    assert!(state.hidden_workspaces.is_empty());
    assert!(
        matches!(&outcome.actions[..], [ClientShellAction::Endpoint { request, .. }]
        if matches!(&request.method, crate::api::schema::Method::WorkspaceFocus(target) if target.workspace_id == "ws_2"))
    );
    let mut projected = state.snapshot.as_deref().unwrap().clone();
    projected.revision += 1;
    projected.focused_workspace_id = Some("ws_2".into());
    for workspace in &mut projected.workspaces {
        workspace.focused = workspace.workspace_id == "ws_2";
    }
    state.set_snapshot(Box::new(projected));
    assert!(state.hidden_workspaces["local"].contains("ws_1"));
    state.hide_workspace("ws_3".into(), &mut outcome);
    let mut last = ClientShellInput::default();
    state.hide_workspace("ws_2".into(), &mut last);
    assert!(last.actions.is_empty());
    assert!(!state.hidden_workspaces["local"].contains("ws_2"));
    assert!(state
        .endpoint_error
        .as_deref()
        .unwrap()
        .contains("one workspace visible"));
}

#[test]
fn hidden_workspaces_are_skipped_by_navigation_and_collapsed_sidebar() {
    let mut state = create_state();
    state.hide_workspace("ws_2".into(), &mut ClientShellInput::default());
    assert!(
        matches!(state.endpoint_method_for_action(crate::input::KeybindAction::NextWorkspace),
        Some(crate::api::schema::Method::WorkspaceFocus(target)) if target.workspace_id == "ws_3")
    );
    assert!(state
        .endpoint_method_for_action(crate::input::KeybindAction::NextAgent)
        .is_none());
    state.sidebar_collapsed = true;
    state.compose(106, 32).unwrap();
    assert_eq!(state.hits.workspaces.len(), 2);
    assert!(state
        .hits
        .workspaces
        .iter()
        .all(|hit| hit.workspace_id != "ws_2"));
    assert!(state.hits.agents.is_empty());
}

#[test]
fn hiding_parent_leaves_worktree_visible_and_prunes_closed_hidden_workspaces() {
    let mut state = create_state();
    let mut projected = state.snapshot.as_deref().unwrap().clone();
    projected.workspaces[1].worktree = Some(ClientShellWorktree {
        key: "repo".into(),
        label: "repo".into(),
        is_linked_worktree: false,
    });
    projected.workspaces[2].worktree = Some(ClientShellWorktree {
        key: "repo".into(),
        label: "repo".into(),
        is_linked_worktree: true,
    });
    state.set_snapshot(Box::new(projected));
    state.collapsed_groups.insert("repo".into());
    state.hide_workspace("ws_2".into(), &mut ClientShellInput::default());
    let entries = state.navigation_workspace_entries(state.snapshot.as_deref().unwrap());
    assert_eq!(entries.len(), 2);
    assert_eq!(entries[1].index, 2);
    assert!(!entries[1].indented);
    let mut projected = state.snapshot.as_deref().unwrap().clone();
    projected
        .workspaces
        .retain(|workspace| workspace.workspace_id != "ws_2");
    state.set_snapshot(Box::new(projected));
    assert!(state.hidden_workspaces["local"].is_empty());
}

#[test]
fn rejected_focus_cancels_hide_without_removing_the_active_workspace() {
    let mut state = create_state();
    let mut outcome = ClientShellInput::default();
    state.hide_workspace("ws_1".into(), &mut outcome);
    let [ClientShellAction::Endpoint { request, .. }] = &outcome.actions[..] else {
        panic!("focus action")
    };
    state.handle_endpoint_result(
        "boot-1",
        &request.id,
        Err(ClientShellEndpointError {
            code: Some("invalid_params".into()),
            message: "rejected".into(),
        }),
    );
    assert!(state.pending_workspace_hide.is_none());
    assert!(state.hidden_workspaces.is_empty());
}

#[test]
fn notification_click_restores_hidden_workspace_and_keeps_notifications_enabled() {
    let mut state = create_state();
    state.config.toast_delay_seconds = 0;
    state.config.toast_delivery = crate::config::ToastDelivery::Herdr;
    state.hide_workspace("ws_2".into(), &mut ClientShellInput::default());
    let now = std::time::Instant::now();
    state.receive_notification(
        &ClientEndpointId::Local,
        SemanticNotification {
            kind: SemanticNotificationKind::Custom,
            title: "background notice".into(),
            body: None,
            sound: None,
            agent: None,
            workspace_id: Some("ws_2".into()),
            tab_id: Some("tab_2".into()),
            pane_id: Some("pane_2".into()),
            position: None,
        },
        now,
    );
    state.tick_notifications(now);
    assert!(state.visible_notification.is_some());
    let mut outcome = ClientShellInput::default();
    state.focus_visible_notification(&mut outcome);
    assert!(!state.hidden_workspaces["local"].contains("ws_2"));
    assert!(
        matches!(&outcome.actions[..], [ClientShellAction::Endpoint { request, .. }]
        if matches!(&request.method, crate::api::schema::Method::PaneFocus(target) if target.pane_id == "pane_2"))
    );
}

#[test]
fn hidden_tree_is_available_in_mobile_switcher_and_hide_is_keyboard_accessible() {
    let mut state = create_state();
    state.mode = ClientShellMode::Navigate;
    state.navigate_workspace_id = Some("ws_2".into());
    state.handle_input_bytes(b"\x1b[21;2~");
    assert!(matches!(
        state.overlay,
        Some(ClientShellOverlay::ContextMenu(_))
    ));
    state.handle_input_bytes(b"\x1b[B\r");
    assert!(state.hidden_workspaces["local"].contains("ws_2"));
    state.hidden_workspaces_expanded = true;
    state.mode = ClientShellMode::Navigate;
    state.compose(44, 40).unwrap();
    assert!(state
        .hits
        .mobile_targets
        .iter()
        .any(|(_, target)| *target == ClientMobileTarget::ToggleHiddenWorkspaces));
    assert!(state.hits.mobile_targets.iter().any(|(_, target)| matches!(target, ClientMobileTarget::Workspace { workspace_id, .. } if workspace_id == "ws_2")));
}
