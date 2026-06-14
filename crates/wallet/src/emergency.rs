//! Emergency stop registry (`docs/wallet.md` §14.1 `EmergencyStop`).

use borsh::{BorshDeserialize, BorshSerialize};

use crate::types::{Scope, TaskId, WorkspaceId};

/// Blast radius: `Global` stops everything; narrower levels stop caps whose `Scope` lies under the rule.
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub enum EmergencyLevel {
    Global,
    Workspace {
        workspace_id: WorkspaceId,
    },
    Project {
        workspace_id: WorkspaceId,
        project_id: u64,
    },
    Task {
        workspace_id: WorkspaceId,
        project_id: u64,
        task_id: TaskId,
    },
}

#[derive(Clone, Debug, Default, BorshSerialize, BorshDeserialize)]
pub struct EmergencyRegistry {
    pub rules: Vec<EmergencyLevel>,
}

impl EmergencyRegistry {
    pub fn activate(&mut self, level: EmergencyLevel) {
        self.rules.push(level);
    }

    pub fn clear(&mut self) {
        self.rules.clear();
    }

    /// Whether a capability `Scope` is covered by any active emergency rule.
    pub fn blocks(&self, cap: &Scope) -> bool {
        self.rules.iter().any(|r| rule_covers(r, cap))
    }
}

fn rule_covers(rule: &EmergencyLevel, cap: &Scope) -> bool {
    match rule {
        EmergencyLevel::Global => true,
        EmergencyLevel::Workspace { workspace_id } => cap.workspace_id == Some(*workspace_id),
        EmergencyLevel::Project {
            workspace_id,
            project_id,
        } => cap.workspace_id == Some(*workspace_id) && cap.project_id == Some(*project_id),
        EmergencyLevel::Task {
            workspace_id,
            project_id,
            task_id,
        } => {
            cap.workspace_id == Some(*workspace_id)
                && cap.project_id == Some(*project_id)
                && cap.task_id == Some(*task_id)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::ToolClass;

    fn scope(ws: Option<WorkspaceId>, proj: Option<u64>, task: Option<TaskId>) -> Scope {
        Scope {
            workspace_id: ws,
            project_id: proj,
            task_id: task,
            tool_class_mask: ToolClass::Browser.bit(),
            providers: None,
        }
    }

    #[test]
    fn workspace_rule_blocks_matching_scope() {
        let mut reg = EmergencyRegistry::default();
        reg.activate(EmergencyLevel::Workspace { workspace_id: 9 });
        assert!(reg.blocks(&scope(Some(9), Some(1), Some(2))));
        assert!(!reg.blocks(&scope(Some(8), None, None)));
    }
}
