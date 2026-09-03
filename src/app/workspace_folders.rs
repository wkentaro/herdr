use std::collections::HashMap;

use crate::workspace_layout::WorkspaceLayoutError;

use super::state::AppState;

impl AppState {
    pub(crate) fn toggle_workspace_folder(&mut self, folder_id: &str) -> bool {
        if self
            .workspace_layout
            .find_folder(folder_id)
            .is_none_or(|folder| folder.workspace_ids.is_empty())
        {
            return false;
        }
        if !self.collapsed_folder_ids.remove(folder_id) {
            self.collapsed_folder_ids.insert(folder_id.to_string());
        }
        true
    }

    pub(crate) fn normalize_workspace_layout(&mut self) {
        let workspace_ids = self
            .workspaces
            .iter()
            .map(|workspace| workspace.id.clone())
            .collect::<Vec<_>>();
        self.workspace_layout.normalize(&workspace_ids);
        self.apply_workspace_layout_order();
    }

    pub(crate) fn append_workspace_to_layout(&mut self, workspace_id: String) {
        self.workspace_layout.append_workspace(workspace_id);
    }

    pub(crate) fn remove_workspace_from_layout(&mut self, workspace_id: &str) {
        self.workspace_layout.remove_workspace(workspace_id);
    }

    pub(crate) fn push_workspace(&mut self, workspace: crate::workspace::Workspace) -> usize {
        self.normalize_workspace_layout();
        self.append_workspace_to_layout(workspace.id.clone());
        self.workspaces.push(workspace);
        self.workspaces.len() - 1
    }

    pub(crate) fn remove_workspace_at(
        &mut self,
        workspace_index: usize,
    ) -> crate::workspace::Workspace {
        let workspace_id = self.workspaces[workspace_index].id.clone();
        self.remove_workspace_from_layout(&workspace_id);
        self.workspaces.remove(workspace_index)
    }

    pub(crate) fn create_workspace_folder(
        &mut self,
        name: String,
    ) -> Result<String, WorkspaceLayoutError> {
        self.normalize_workspace_layout();
        let folder_id = self.workspace_layout.create_folder(name)?.id.clone();
        self.mark_session_dirty();
        Ok(folder_id)
    }

    pub(crate) fn rename_workspace_folder(
        &mut self,
        folder_id: &str,
        name: String,
    ) -> Result<bool, WorkspaceLayoutError> {
        self.normalize_workspace_layout();
        let changed = self.workspace_layout.rename_folder(folder_id, name)?;
        if changed {
            self.mark_session_dirty();
        }
        Ok(changed)
    }

    pub(crate) fn delete_workspace_folder(
        &mut self,
        folder_id: &str,
    ) -> Result<(), WorkspaceLayoutError> {
        self.normalize_workspace_layout();
        self.workspace_layout.delete_folder(folder_id)?;
        self.collapsed_folder_ids.remove(folder_id);
        if self.selected_folder_id.as_deref() == Some(folder_id) {
            self.selected_folder_id = None;
        }
        self.mark_session_dirty();
        Ok(())
    }

    pub(crate) fn move_workspace_folder(
        &mut self,
        folder_id: &str,
        insert_index: usize,
    ) -> Result<bool, WorkspaceLayoutError> {
        self.normalize_workspace_layout();
        let changed = self.workspace_layout.move_folder(folder_id, insert_index)?;
        if changed {
            self.apply_workspace_layout_order();
            self.mark_session_dirty();
        }
        Ok(changed)
    }

    pub(crate) fn place_workspace_in_layout(
        &mut self,
        workspace_id: &str,
        folder_id: Option<&str>,
        insert_index: usize,
    ) -> Result<bool, WorkspaceLayoutError> {
        self.normalize_workspace_layout();
        let changed =
            self.workspace_layout
                .place_workspace(workspace_id, folder_id, insert_index)?;
        if changed {
            self.apply_workspace_layout_order();
            self.mark_session_dirty();
        }
        Ok(changed)
    }

    pub(crate) fn place_workspaces_at_layout_root(
        &mut self,
        workspace_ids: &[String],
        before_workspace_id: Option<&str>,
    ) -> Result<bool, WorkspaceLayoutError> {
        self.normalize_workspace_layout();
        let changed = self
            .workspace_layout
            .place_workspaces_at_root(workspace_ids, before_workspace_id)?;
        if changed {
            self.apply_workspace_layout_order();
            self.mark_session_dirty();
        }
        Ok(changed)
    }

    fn apply_workspace_layout_order(&mut self) {
        let unique_workspace_ids = self
            .workspaces
            .iter()
            .map(|workspace| workspace.id.as_str())
            .collect::<std::collections::HashSet<_>>();
        if unique_workspace_ids.len() != self.workspaces.len() {
            return;
        }
        let ordered_ids = self
            .workspace_layout
            .collect_ordered_workspace_ids()
            .into_iter()
            .map(str::to_string)
            .collect::<Vec<_>>();
        if self
            .workspaces
            .iter()
            .map(|workspace| workspace.id.as_str())
            .eq(ordered_ids.iter().map(String::as_str))
        {
            return;
        }

        let active_id = self
            .active
            .and_then(|index| self.workspaces.get(index))
            .map(|workspace| workspace.id.clone());
        let selected_id = self
            .workspaces
            .get(self.selected)
            .map(|workspace| workspace.id.clone());
        let mut workspaces_by_id = self
            .workspaces
            .drain(..)
            .map(|workspace| (workspace.id.clone(), workspace))
            .collect::<HashMap<_, _>>();
        self.workspaces = ordered_ids
            .into_iter()
            .filter_map(|workspace_id| workspaces_by_id.remove(&workspace_id))
            .collect();
        self.active = active_id.as_deref().and_then(|id| {
            self.workspaces
                .iter()
                .position(|workspace| workspace.id == id)
        });
        self.selected = selected_id
            .as_deref()
            .and_then(|id| {
                self.workspaces
                    .iter()
                    .position(|workspace| workspace.id == id)
            })
            .unwrap_or(0);
    }
}

#[cfg(test)]
mod tests {
    use crate::workspace::Workspace;
    use crate::workspace_layout::{WorkspaceFolder, WorkspaceLayoutItem};

    use super::AppState;

    #[test]
    fn placing_workspace_reorders_canonical_workspaces_without_changing_focus() {
        let mut state = AppState::test_new();
        state.workspaces = vec![
            Workspace::test_new("one"),
            Workspace::test_new("two"),
            Workspace::test_new("three"),
        ];
        state.active = Some(2);
        state.selected = 1;
        state.normalize_workspace_layout();
        let folder_id = state.create_workspace_folder("project".into()).unwrap();
        let moved_id = state.workspaces[1].id.clone();
        let active_id = state.workspaces[2].id.clone();

        assert_eq!(
            state.place_workspace_in_layout(&moved_id, Some(&folder_id), 0),
            Ok(true)
        );
        assert_eq!(state.move_workspace_folder(&folder_id, 0), Ok(true));

        assert_eq!(state.workspaces[0].id, moved_id);
        assert_eq!(
            state.active.map(|index| &state.workspaces[index].id),
            Some(&active_id)
        );
        assert_eq!(state.workspaces[state.selected].display_name(), "two");
    }

    #[test]
    fn deleting_folder_keeps_children_and_their_order() {
        let mut state = AppState::test_new();
        state.workspaces = vec![Workspace::test_new("one"), Workspace::test_new("two")];
        let workspace_ids = state
            .workspaces
            .iter()
            .map(|workspace| workspace.id.clone())
            .collect::<Vec<_>>();
        state.workspace_layout.items = vec![WorkspaceLayoutItem::Folder(WorkspaceFolder {
            id: "f1".into(),
            name: "project".into(),
            workspace_ids: workspace_ids.clone(),
        })];

        assert_eq!(state.delete_workspace_folder("f1"), Ok(()));

        assert_eq!(
            state.workspace_layout.items,
            workspace_ids
                .into_iter()
                .map(WorkspaceLayoutItem::Workspace)
                .collect::<Vec<_>>()
        );
        assert_eq!(state.workspaces.len(), 2);
    }
}
