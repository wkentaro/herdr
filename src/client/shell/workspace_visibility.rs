use super::*;

pub(super) type HiddenWorkspaces = HashMap<String, HashSet<String>>;

pub(super) fn get_hidden_workspace_ids<'a>(
    hidden: &'a HiddenWorkspaces,
    endpoint: &ClientEndpointId,
) -> &'a HashSet<String> {
    static EMPTY: std::sync::LazyLock<HashSet<String>> = std::sync::LazyLock::new(HashSet::new);
    if hidden.is_empty() {
        return &EMPTY;
    }
    hidden.get(&endpoint.storage_key()).unwrap_or(&EMPTY)
}

impl ClientShellState {
    pub(super) fn is_focused_workspace_hidden(&self) -> bool {
        self.snapshot
            .as_deref()
            .and_then(|snapshot| snapshot.focused_workspace_id.as_ref())
            .is_some_and(|workspace| {
                get_hidden_workspace_ids(&self.hidden_workspaces, &self.active_endpoint_id)
                    .contains(workspace)
            })
    }

    pub(crate) fn redirect_hidden_workspace_focus(&mut self, outcome: &mut ClientShellInput) {
        if self.hidden_focus_redirect_attempted || !self.is_focused_workspace_hidden() {
            return;
        }
        let target = self
            .endpoints
            .iter()
            .filter(|endpoint| endpoint.status == ClientEndpointStatus::Online)
            .find_map(|endpoint| {
                let hidden =
                    get_hidden_workspace_ids(&self.hidden_workspaces, &endpoint.endpoint_id);
                endpoint
                    .snapshot
                    .as_deref()?
                    .workspaces
                    .iter()
                    .find(|workspace| !hidden.contains(&workspace.workspace_id))
                    .map(|workspace| (endpoint.endpoint_id.clone(), workspace.workspace_id.clone()))
            });
        if let Some((endpoint, workspace)) = target {
            self.hidden_focus_redirect_attempted = true;
            self.focus_or_activate(
                endpoint,
                ClientEndpointFocusTarget::Workspace(workspace),
                outcome,
            );
        }
    }

    pub(super) fn prune_hidden_workspaces(
        &mut self,
        endpoint: &ClientEndpointId,
        snapshot: &ClientShellSnapshot,
    ) {
        let Some(hidden) = self
            .hidden_workspaces
            .get_mut(&endpoint.storage_key())
            .filter(|hidden| !hidden.is_empty())
        else {
            return;
        };
        let existing = snapshot
            .workspaces
            .iter()
            .map(|workspace| workspace.workspace_id.as_str())
            .collect::<HashSet<_>>();
        let previous = hidden.len();
        hidden.retain(|workspace| existing.contains(workspace.as_str()));
        if hidden.len() != previous {
            self.persist_chrome_preferences(&mut ClientShellInput::default());
        }
    }

    pub(super) fn hide_workspace(&mut self, workspace_id: String, outcome: &mut ClientShellInput) {
        let Some(snapshot) = self.snapshot.as_deref() else {
            return;
        };
        if !snapshot
            .workspaces
            .iter()
            .any(|workspace| workspace.workspace_id == workspace_id)
        {
            return;
        }
        let next = self
            .endpoints
            .iter()
            .filter(|endpoint| endpoint.status == ClientEndpointStatus::Online)
            .find_map(|endpoint| {
                let hidden =
                    get_hidden_workspace_ids(&self.hidden_workspaces, &endpoint.endpoint_id);
                endpoint
                    .snapshot
                    .as_deref()?
                    .workspaces
                    .iter()
                    .find(|workspace| {
                        !hidden.contains(&workspace.workspace_id)
                            && (endpoint.endpoint_id != self.active_endpoint_id
                                || workspace.workspace_id != workspace_id)
                    })
                    .map(|workspace| (endpoint.endpoint_id.clone(), workspace.workspace_id.clone()))
            });
        let Some((endpoint_id, next_workspace)) = next else {
            self.pending_workspace_hide = Some((self.active_endpoint_id.clone(), workspace_id));
            if !self.push_endpoint_method_with_kind(
                crate::api::schema::Method::WorkspaceCreateDefault(
                    crate::api::schema::EmptyParams::default(),
                ),
                PendingEndpointKind::Generic,
                outcome,
            ) {
                self.pending_workspace_hide = None;
            }
            self.navigate_workspace_id = None;
            outcome.repaint = true;
            return;
        };
        if snapshot.focused_workspace_id.as_deref() == Some(&workspace_id) {
            self.pending_workspace_hide = Some((self.active_endpoint_id.clone(), workspace_id));
            if !self.focus_or_activate(
                endpoint_id,
                ClientEndpointFocusTarget::Workspace(next_workspace),
                outcome,
            ) {
                self.pending_workspace_hide = None;
            }
        } else {
            self.hidden_workspaces
                .entry(self.active_endpoint_id.storage_key())
                .or_default()
                .insert(workspace_id);
            self.persist_chrome_preferences(outcome);
        }
        self.navigate_workspace_id = None;
        outcome.repaint = true;
    }

    pub(super) fn restore_hidden_workspace(
        &mut self,
        endpoint: &ClientEndpointId,
        workspace_id: &str,
        outcome: &mut ClientShellInput,
    ) {
        if self
            .hidden_workspaces
            .get_mut(&endpoint.storage_key())
            .is_some_and(|hidden| hidden.remove(workspace_id))
        {
            self.reveal_focused_workspace = true;
            self.persist_chrome_preferences(outcome);
            outcome.repaint = true;
        }
    }

    pub(super) fn reconcile_workspace_visibility(&mut self, focus_changed: bool) {
        let Some(snapshot) = self.snapshot.as_deref() else {
            return;
        };
        let mut changed = false;
        if self
            .pending_workspace_hide
            .as_ref()
            .is_some_and(|(endpoint, workspace)| {
                endpoint != &self.active_endpoint_id
                    || snapshot.focused_workspace_id.as_deref() != Some(workspace)
            })
        {
            if let Some((endpoint, workspace)) = self.pending_workspace_hide.take() {
                changed |= self
                    .hidden_workspaces
                    .entry(endpoint.storage_key())
                    .or_default()
                    .insert(workspace);
            }
        }
        if let Some(hidden) = self
            .hidden_workspaces
            .get_mut(&self.active_endpoint_id.storage_key())
        {
            // Only an actual focus change expresses intent; a newly attached client inherits server defaults.
            if focus_changed {
                if let Some(focused) = snapshot.focused_workspace_id.as_ref() {
                    changed |= hidden.remove(focused);
                }
            }
        }
        if !self.is_focused_workspace_hidden() {
            self.hidden_focus_redirect_attempted = false;
        }
        if changed {
            self.persist_chrome_preferences(&mut ClientShellInput::default());
        }
    }

    pub(super) fn handle_hidden_workspace_click(
        &mut self,
        point: (u16, u16),
        outcome: &mut ClientShellInput,
    ) -> bool {
        if super::contains(self.hits.hidden_workspaces_toggle, point) {
            self.hidden_workspaces_expanded = !self.hidden_workspaces_expanded;
            outcome.repaint = true;
            return true;
        }
        let target = self
            .hits
            .hidden_workspaces
            .iter()
            .find(|(rect, _, _)| super::contains(*rect, point))
            .map(|(_, endpoint, workspace)| (endpoint.clone(), workspace.clone()));
        if let Some((endpoint, workspace)) = target {
            self.focus_or_activate(
                endpoint,
                ClientEndpointFocusTarget::Workspace(workspace),
                outcome,
            );
            return true;
        }
        false
    }
}

pub(super) enum HiddenWorkspaceRow<'a> {
    Header(usize),
    Workspace(&'a ClientShellEndpoint, &'a ClientShellWorkspace),
}

pub(super) fn collect_hidden_workspace_rows<'a>(
    endpoints: &'a [ClientShellEndpoint],
    hidden: &HiddenWorkspaces,
    expanded: bool,
) -> Vec<HiddenWorkspaceRow<'a>> {
    let mut rows = Vec::new();
    let mut count = 0;
    for endpoint in endpoints {
        let ids = get_hidden_workspace_ids(hidden, &endpoint.endpoint_id);
        if ids.is_empty() {
            continue;
        }
        if let Some(snapshot) = endpoint.snapshot.as_deref() {
            for workspace in &snapshot.workspaces {
                if ids.contains(&workspace.workspace_id) {
                    count += 1;
                    if expanded {
                        rows.push(HiddenWorkspaceRow::Workspace(endpoint, workspace));
                    }
                }
            }
        }
    }
    if count > 0 {
        rows.insert(0, HiddenWorkspaceRow::Header(count));
    }
    rows
}

pub(super) fn render_hidden_workspace_row(
    buffer: &mut Buffer,
    rect: Rect,
    row: &HiddenWorkspaceRow<'_>,
    expanded: bool,
    palette: &Palette,
    hits: &mut ShellHitMap,
) {
    let label = match row {
        HiddenWorkspaceRow::Header(count) => {
            hits.hidden_workspaces_toggle = rect;
            format!(" {} Hidden ({count})", if expanded { "▾" } else { "▸" })
        }
        HiddenWorkspaceRow::Workspace(endpoint, workspace) => {
            hits.hidden_workspaces.push((
                rect,
                endpoint.endpoint_id.clone(),
                workspace.workspace_id.clone(),
            ));
            format!("   {} · {}", workspace.label, endpoint.label)
        }
    };
    render::put_text(
        buffer,
        rect.x,
        rect.y,
        rect.width,
        &label,
        Style::default().fg(palette.overlay0),
    );
}
