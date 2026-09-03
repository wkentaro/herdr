use std::collections::HashSet;
use std::sync::atomic::{AtomicU64, Ordering};

use serde::{Deserialize, Serialize};

static NEXT_FOLDER_ID: AtomicU64 = AtomicU64::new(1);

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorkspaceFolder {
    pub id: String,
    pub name: String,
    pub workspace_ids: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum WorkspaceLayoutItem {
    Workspace(String),
    Folder(WorkspaceFolder),
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorkspaceLayout {
    pub items: Vec<WorkspaceLayoutItem>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WorkspaceLayoutError {
    EmptyFolderName,
    FolderNotFound,
    WorkspaceNotFound,
    InsertIndexOutOfBounds,
}

impl WorkspaceLayout {
    pub fn normalize(&mut self, workspace_ids: &[String]) {
        let valid_ids = workspace_ids
            .iter()
            .map(String::as_str)
            .collect::<HashSet<_>>();
        let mut seen_workspace_ids = HashSet::new();
        let mut seen_folder_ids = HashSet::new();

        self.items.retain_mut(|item| match item {
            WorkspaceLayoutItem::Workspace(workspace_id) => {
                valid_ids.contains(workspace_id.as_str())
                    && seen_workspace_ids.insert(workspace_id.clone())
            }
            WorkspaceLayoutItem::Folder(folder) => {
                if !seen_folder_ids.insert(folder.id.clone()) {
                    return false;
                }
                folder.workspace_ids.retain(|workspace_id| {
                    valid_ids.contains(workspace_id.as_str())
                        && seen_workspace_ids.insert(workspace_id.clone())
                });
                true
            }
        });

        self.items.extend(
            workspace_ids
                .iter()
                .filter(|workspace_id| seen_workspace_ids.insert((*workspace_id).clone()))
                .cloned()
                .map(WorkspaceLayoutItem::Workspace),
        );
        reserve_folder_ids(&self.items);
    }

    pub fn collect_ordered_workspace_ids(&self) -> Vec<&str> {
        let mut workspace_ids = Vec::new();
        for item in &self.items {
            match item {
                WorkspaceLayoutItem::Workspace(workspace_id) => {
                    workspace_ids.push(workspace_id.as_str())
                }
                WorkspaceLayoutItem::Folder(folder) => {
                    workspace_ids.extend(folder.workspace_ids.iter().map(String::as_str))
                }
            }
        }
        workspace_ids
    }

    pub fn create_folder(
        &mut self,
        name: String,
    ) -> Result<&WorkspaceFolder, WorkspaceLayoutError> {
        let name = normalize_folder_name(name)?;
        self.items
            .push(WorkspaceLayoutItem::Folder(WorkspaceFolder {
                id: generate_folder_id(),
                name,
                workspace_ids: Vec::new(),
            }));
        let Some(WorkspaceLayoutItem::Folder(folder)) = self.items.last() else {
            unreachable!("the appended layout item is a folder");
        };
        Ok(folder)
    }

    pub fn rename_folder(
        &mut self,
        folder_id: &str,
        name: String,
    ) -> Result<bool, WorkspaceLayoutError> {
        let name = normalize_folder_name(name)?;
        let Some(folder) = self.find_folder_mut(folder_id) else {
            return Err(WorkspaceLayoutError::FolderNotFound);
        };
        if folder.name == name {
            return Ok(false);
        }
        folder.name = name;
        Ok(true)
    }

    pub fn delete_folder(&mut self, folder_id: &str) -> Result<(), WorkspaceLayoutError> {
        let Some(index) = self.find_folder_index(folder_id) else {
            return Err(WorkspaceLayoutError::FolderNotFound);
        };
        let WorkspaceLayoutItem::Folder(folder) = self.items.remove(index) else {
            unreachable!("the resolved layout item is a folder");
        };
        self.items.splice(
            index..index,
            folder
                .workspace_ids
                .into_iter()
                .map(WorkspaceLayoutItem::Workspace),
        );
        Ok(())
    }

    pub fn move_folder(
        &mut self,
        folder_id: &str,
        insert_index: usize,
    ) -> Result<bool, WorkspaceLayoutError> {
        let Some(source_index) = self.find_folder_index(folder_id) else {
            return Err(WorkspaceLayoutError::FolderNotFound);
        };
        if insert_index > self.items.len() {
            return Err(WorkspaceLayoutError::InsertIndexOutOfBounds);
        }
        let target_index = if source_index < insert_index {
            insert_index.saturating_sub(1)
        } else {
            insert_index
        };
        if source_index == target_index {
            return Ok(false);
        }
        let folder = self.items.remove(source_index);
        self.items.insert(target_index, folder);
        Ok(true)
    }

    pub fn place_workspace(
        &mut self,
        workspace_id: &str,
        folder_id: Option<&str>,
        insert_index: usize,
    ) -> Result<bool, WorkspaceLayoutError> {
        let Some((source_item_index, source_child_index)) =
            self.find_workspace_position(workspace_id)
        else {
            return Err(WorkspaceLayoutError::WorkspaceNotFound);
        };
        if folder_id.is_some_and(|id| self.find_folder(id).is_none()) {
            return Err(WorkspaceLayoutError::FolderNotFound);
        }

        let destination_len = folder_id
            .and_then(|id| self.find_folder(id))
            .map_or_else(|| self.items.len(), |folder| folder.workspace_ids.len());
        if insert_index > destination_len {
            return Err(WorkspaceLayoutError::InsertIndexOutOfBounds);
        }

        let same_folder = source_child_index.is_some()
            && folder_id.is_some_and(|id| {
                matches!(
                    self.items.get(source_item_index),
                    Some(WorkspaceLayoutItem::Folder(folder)) if folder.id == id
                )
            });
        let target_index = if (same_folder && source_child_index.is_some_and(|i| i < insert_index))
            || (folder_id.is_none()
                && source_child_index.is_none()
                && source_item_index < insert_index)
        {
            insert_index.saturating_sub(1)
        } else {
            insert_index
        };

        if (same_folder && source_child_index == Some(target_index))
            || (folder_id.is_none()
                && source_child_index.is_none()
                && source_item_index == target_index)
        {
            return Ok(false);
        }

        if let Some(child_index) = source_child_index {
            let Some(WorkspaceLayoutItem::Folder(folder)) = self.items.get_mut(source_item_index)
            else {
                unreachable!("the workspace child belongs to a folder");
            };
            folder.workspace_ids.remove(child_index);
        } else {
            self.items.remove(source_item_index);
        }

        if let Some(folder_id) = folder_id {
            let Some(folder) = self.find_folder_mut(folder_id) else {
                return Err(WorkspaceLayoutError::FolderNotFound);
            };
            folder.workspace_ids.insert(
                target_index.min(folder.workspace_ids.len()),
                workspace_id.to_string(),
            );
        } else {
            self.items.insert(
                target_index.min(self.items.len()),
                WorkspaceLayoutItem::Workspace(workspace_id.to_string()),
            );
        }
        Ok(true)
    }

    pub fn place_workspaces_at_root(
        &mut self,
        workspace_ids: &[String],
        before_workspace_id: Option<&str>,
    ) -> Result<bool, WorkspaceLayoutError> {
        if workspace_ids
            .iter()
            .any(|workspace_id| self.find_workspace_position(workspace_id).is_none())
            || before_workspace_id
                .is_some_and(|workspace_id| self.find_workspace_position(workspace_id).is_none())
        {
            return Err(WorkspaceLayoutError::WorkspaceNotFound);
        }

        let previous = self.clone();
        for workspace_id in workspace_ids {
            self.remove_workspace(workspace_id);
        }
        let insert_index = before_workspace_id
            .and_then(|workspace_id| self.find_workspace_root_index(workspace_id))
            .unwrap_or(self.items.len());
        self.items.splice(
            insert_index..insert_index,
            workspace_ids
                .iter()
                .cloned()
                .map(WorkspaceLayoutItem::Workspace),
        );
        Ok(*self != previous)
    }

    pub fn append_workspace(&mut self, workspace_id: String) {
        if self.find_workspace_position(&workspace_id).is_none() {
            self.items
                .push(WorkspaceLayoutItem::Workspace(workspace_id));
        }
    }

    pub fn remove_workspace(&mut self, workspace_id: &str) -> bool {
        let Some((item_index, child_index)) = self.find_workspace_position(workspace_id) else {
            return false;
        };
        if let Some(child_index) = child_index {
            let Some(WorkspaceLayoutItem::Folder(folder)) = self.items.get_mut(item_index) else {
                return false;
            };
            folder.workspace_ids.remove(child_index);
        } else {
            self.items.remove(item_index);
        }
        true
    }

    pub fn find_folder(&self, folder_id: &str) -> Option<&WorkspaceFolder> {
        self.items.iter().find_map(|item| match item {
            WorkspaceLayoutItem::Folder(folder) if folder.id == folder_id => Some(folder),
            _ => None,
        })
    }

    pub fn find_workspace_root_index(&self, workspace_id: &str) -> Option<usize> {
        self.find_workspace_position(workspace_id)
            .map(|(item_index, _)| item_index)
    }

    fn find_folder_mut(&mut self, folder_id: &str) -> Option<&mut WorkspaceFolder> {
        self.items.iter_mut().find_map(|item| match item {
            WorkspaceLayoutItem::Folder(folder) if folder.id == folder_id => Some(folder),
            _ => None,
        })
    }

    fn find_folder_index(&self, folder_id: &str) -> Option<usize> {
        self.items.iter().position(
            |item| matches!(item, WorkspaceLayoutItem::Folder(folder) if folder.id == folder_id),
        )
    }

    fn find_workspace_position(&self, workspace_id: &str) -> Option<(usize, Option<usize>)> {
        self.items
            .iter()
            .enumerate()
            .find_map(|(item_index, item)| match item {
                WorkspaceLayoutItem::Workspace(candidate) if candidate == workspace_id => {
                    Some((item_index, None))
                }
                WorkspaceLayoutItem::Folder(folder) => folder
                    .workspace_ids
                    .iter()
                    .position(|candidate| candidate == workspace_id)
                    .map(|child_index| (item_index, Some(child_index))),
                _ => None,
            })
    }
}

fn generate_folder_id() -> String {
    format!("f{}", NEXT_FOLDER_ID.fetch_add(1, Ordering::Relaxed))
}

fn normalize_folder_name(name: String) -> Result<String, WorkspaceLayoutError> {
    let name = name.trim().to_string();
    if name.is_empty() {
        return Err(WorkspaceLayoutError::EmptyFolderName);
    }
    Ok(name)
}

fn reserve_folder_ids(items: &[WorkspaceLayoutItem]) {
    let Some(next) = items
        .iter()
        .filter_map(|item| match item {
            WorkspaceLayoutItem::Folder(folder) => folder.id.strip_prefix('f')?.parse::<u64>().ok(),
            WorkspaceLayoutItem::Workspace(_) => None,
        })
        .max()
        .and_then(|max| max.checked_add(1))
    else {
        return;
    };
    NEXT_FOLDER_ID.fetch_max(next, Ordering::Relaxed);
}

#[cfg(test)]
mod tests {
    use super::{WorkspaceFolder, WorkspaceLayout, WorkspaceLayoutItem};

    fn workspace(id: &str) -> WorkspaceLayoutItem {
        WorkspaceLayoutItem::Workspace(id.to_string())
    }

    fn folder(id: &str, workspace_ids: &[&str]) -> WorkspaceLayoutItem {
        WorkspaceLayoutItem::Folder(WorkspaceFolder {
            id: id.to_string(),
            name: id.to_string(),
            workspace_ids: workspace_ids.iter().map(|id| (*id).to_string()).collect(),
        })
    }

    #[test]
    fn normalizing_removes_stale_duplicates_and_appends_missing_workspaces() {
        let mut layout = WorkspaceLayout {
            items: vec![
                workspace("w2"),
                folder("f1", &["w1", "w2", "gone"]),
                workspace("w1"),
            ],
        };

        layout.normalize(&["w1".into(), "w2".into(), "w3".into()]);

        assert_eq!(
            layout.items,
            vec![workspace("w2"), folder("f1", &["w1"]), workspace("w3")]
        );
        assert_eq!(
            layout.collect_ordered_workspace_ids(),
            vec!["w2", "w1", "w3"]
        );
    }

    #[test]
    fn deleting_folder_dissolves_it_in_place() {
        let mut layout = WorkspaceLayout {
            items: vec![
                workspace("w1"),
                folder("f1", &["w2", "w3"]),
                workspace("w4"),
            ],
        };

        assert_eq!(layout.delete_folder("f1"), Ok(()));

        assert_eq!(
            layout.items,
            vec![
                workspace("w1"),
                workspace("w2"),
                workspace("w3"),
                workspace("w4")
            ]
        );
    }

    #[test]
    fn placing_workspace_moves_between_root_and_folders() {
        let mut layout = WorkspaceLayout {
            items: vec![workspace("w1"), folder("f1", &["w2"]), workspace("w3")],
        };

        assert_eq!(layout.place_workspace("w3", Some("f1"), 1), Ok(true));
        assert_eq!(
            layout.items,
            vec![workspace("w1"), folder("f1", &["w2", "w3"])]
        );
        assert_eq!(layout.place_workspace("w2", None, 0), Ok(true));
        assert_eq!(
            layout.items,
            vec![workspace("w2"), workspace("w1"), folder("f1", &["w3"])]
        );
    }

    #[test]
    fn placing_multiple_workspaces_at_root_preserves_requested_order() {
        let mut layout = WorkspaceLayout {
            items: vec![
                workspace("child"),
                workspace("normal"),
                workspace("parent"),
                workspace("tail"),
            ],
        };

        assert_eq!(
            layout.place_workspaces_at_root(&["parent".into(), "child".into()], Some("tail")),
            Ok(true)
        );
        assert_eq!(
            layout.collect_ordered_workspace_ids(),
            vec!["normal", "parent", "child", "tail"]
        );
    }

    #[test]
    fn folder_names_are_trimmed_and_must_not_be_empty() {
        let mut layout = WorkspaceLayout::default();

        let folder = layout.create_folder("  project  ".into()).unwrap();
        assert_eq!(folder.name, "project");
        assert_eq!(
            layout.create_folder("   ".into()),
            Err(super::WorkspaceLayoutError::EmptyFolderName)
        );
    }
}
