use std::path::PathBuf;

use crate::tree::Layout;

use super::Editor;

fn prepare_panel_focus(editor: &mut Editor) {
    editor.enter_normal_mode();
    if let Some((view, doc)) = try_current!(editor) {
        doc.append_changes_to_history(view);
    }
}

impl Editor {
    pub fn agent_settings(&self) -> crate::agent::AgentSettings {
        self.config.load().agent.clone()
    }

    pub fn integrated_terminal_settings(&self) -> crate::terminal::TerminalSettings {
        self.config.load().integrated_terminal.clone()
    }

    fn sync_terminal_panel_session_id(&mut self) {
        let Some(panel_id) = self.terminal.panel_id else {
            return;
        };
        let Some(active) = self.terminal.active_session.clone() else {
            return;
        };
        if let Some(panel) = self.tree.terminal_panel_mut(panel_id) {
            panel.session_id = active;
        }
    }

    pub fn switch_terminal_session(&mut self, session_id: &str) -> bool {
        if !self.terminal.switch_session(session_id) {
            return false;
        }
        self.sync_terminal_panel_session_id();
        self._refresh();
        true
    }

    /// Agent and terminal stack in one column; either splits beside the editor alone.
    fn auxiliary_split_layout(&self, pair_with_other_auxiliary: bool) -> Layout {
        if pair_with_other_auxiliary {
            Layout::Horizontal
        } else {
            Layout::Vertical
        }
    }

    pub fn open_agent_panel(&mut self) {
        self.open_agent_panel_with_focus(true);
    }

    pub fn open_agent_panel_with_focus(&mut self, focus_panel: bool) {
        if let Some(panel_id) = self.agent.panel_id {
            if focus_panel {
                if self.tree.focus != panel_id {
                    prepare_panel_focus(self);
                }
                self.tree.focus = panel_id;
            }
            self.agent.focus = crate::agent::AgentFocus::Normal;
            return;
        }

        let document_focus = self.tree.focus;
        prepare_panel_focus(self);
        let panel_id = self.tree.split_agent_panel(Layout::Vertical);
        let fraction = self.agent_settings().panel_width_fraction();
        self.tree.set_leaf_weight_fraction(panel_id, fraction);
        self.agent.panel_id = Some(panel_id);
        self.agent.focus = crate::agent::AgentFocus::Normal;
        if focus_panel {
            self.tree.focus = panel_id;
        } else {
            self.tree.focus = document_focus;
        }
        self._refresh();
    }

    pub fn close_agent_panel(&mut self) {
        let Some(panel_id) = self.agent.panel_id.take() else {
            return;
        };
        if self.tree.focus == panel_id {
            self.tree.focus = self.tree.prev();
        }
        if self.tree.contains(panel_id) {
            self.tree.remove(panel_id);
        }
        self.agent.focus = crate::agent::AgentFocus::Normal;
        self._refresh();
    }

    pub fn focus_agent_panel(&mut self) {
        if let Some(panel_id) = self.agent.panel_id {
            if self.tree.focus != panel_id {
                prepare_panel_focus(self);
            }
            self.tree.focus = panel_id;
            self.agent.focus = crate::agent::AgentFocus::Normal;
        }
    }

    pub fn focus_editor_from_agent(&mut self) {
        self.agent.focus = crate::agent::AgentFocus::Normal;
        if self.tree.is_agent_panel(self.tree.focus) {
            self.tree.focus = self.tree.prev();
        }
    }

    pub fn open_terminal_panel(&mut self, session_id: String) {
        if self.terminal.panel_id.is_some() {
            if !self.tree.is_terminal_panel(self.tree.focus) {
                prepare_panel_focus(self);
            }
            self.terminal.switch_session(&session_id);
            self.sync_terminal_panel_session_id();
            if let Some(panel_id) = self.terminal.panel_id {
                self.tree.focus = panel_id;
            }
            self.terminal.focus = crate::terminal::TerminalFocus::Normal;
            self._refresh();
            return;
        }

        let cwd = self
            .last_cwd
            .clone()
            .or_else(|| std::env::current_dir().ok())
            .unwrap_or_else(|| PathBuf::from("."));
        if !self.terminal.sessions.contains_key(&session_id) {
            self.terminal.insert_session(session_id.clone(), cwd);
        } else if !self.terminal.session_order.contains(&session_id) {
            self.terminal.session_order.push(session_id.clone());
        }
        self.terminal.active_session = Some(session_id.clone());

        let agent_open = self.agent.panel_id.is_some();
        let layout = self.auxiliary_split_layout(agent_open);
        if let Some(agent_panel) = self.agent.panel_id {
            self.tree.focus = agent_panel;
        }

        prepare_panel_focus(self);
        let panel_id = self.tree.split_terminal_panel(layout, session_id.clone());
        if layout == Layout::Vertical {
            let fraction = self.agent_settings().panel_width_fraction();
            self.tree.set_leaf_weight_fraction(panel_id, fraction);
        }
        self.terminal.panel_id = Some(panel_id);
        self.terminal.focus = crate::terminal::TerminalFocus::Normal;
        self.tree.focus = panel_id;
        self._refresh();
    }

    pub fn close_terminal_panel(&mut self) {
        let Some(panel_id) = self.terminal.panel_id.take() else {
            return;
        };

        if self.tree.focus == panel_id {
            self.tree.focus = self.tree.prev();
        }
        if self.tree.contains(panel_id) {
            self.tree.remove(panel_id);
        }
        self.terminal.focus = crate::terminal::TerminalFocus::Normal;
        self._refresh();
    }

    pub fn remove_terminal_tab(&mut self, session_id: &str) -> bool {
        let close_panel = self.terminal.remove_session(session_id);
        if close_panel {
            self.close_terminal_panel();
        } else {
            self.sync_terminal_panel_session_id();
            self._refresh();
        }
        close_panel
    }

    pub fn focus_terminal_panel(&mut self) {
        let Some(panel_id) = self.terminal.panel_id else {
            return;
        };

        if self.tree.focus != panel_id {
            prepare_panel_focus(self);
        }
        self.tree.focus = panel_id;
        self.terminal.focus = crate::terminal::TerminalFocus::Normal;
    }

    pub fn focus_editor_from_terminal(&mut self) {
        self.terminal.focus = crate::terminal::TerminalFocus::Normal;
        if self.tree.is_terminal_panel(self.tree.focus) {
            self.tree.focus = self.tree.prev();
        }
    }

    pub fn open_git_panel(&mut self) {
        if let Some(panel_id) = self.git.panel_id {
            if self.tree.focus != panel_id {
                prepare_panel_focus(self);
            }
            self.tree.focus = panel_id;
            return;
        }

        prepare_panel_focus(self);
        let panel_id = self.tree.split_git_panel(Layout::Vertical);
        self.git.panel_id = Some(panel_id);
        self.tree.focus = panel_id;
        self._refresh();
    }

    pub fn close_git_panel(&mut self) {
        let Some(panel_id) = self.git.panel_id.take() else {
            return;
        };
        if self.tree.focus == panel_id {
            self.tree.focus = self.tree.prev();
        }
        if self.tree.contains(panel_id) {
            self.tree.remove(panel_id);
        }
        self._refresh();
    }

    pub fn focus_git_panel(&mut self) {
        if let Some(panel_id) = self.git.panel_id {
            if self.tree.focus != panel_id {
                prepare_panel_focus(self);
            }
            self.tree.focus = panel_id;
        }
    }

    pub fn focus_editor_from_git(&mut self) {
        if self.tree.is_git_panel(self.tree.focus) {
            let focus = self
                .tree
                .views()
                .map(|(view, _)| view.id)
                .next()
                .unwrap_or(self.tree.focus);
            self.tree.focus = focus;
        }
    }

    pub fn git_cwd(&self) -> PathBuf {
        self.last_cwd
            .clone()
            .or_else(|| std::env::current_dir().ok())
            .unwrap_or_else(|| PathBuf::from("."))
    }

    pub fn apply_git_status(&mut self, entries: Vec<helix_vcs::GitStatusEntry>) {
        self.git.entries = entries;
        self.git.loading = false;
        self.git.error = None;
        if self.git.selection.is_none() {
            if self.git.unstaged().next().is_some() {
                self.git.selection = Some(crate::git::GitSelection {
                    section: helix_vcs::StagingSection::Unstaged,
                    index: 0,
                });
            } else if self.git.staged().next().is_some() {
                self.git.selection = Some(crate::git::GitSelection {
                    section: helix_vcs::StagingSection::Staged,
                    index: 0,
                });
            }
        }
        self._refresh();
    }

    pub fn set_git_error(&mut self, error: String) {
        self.git.loading = false;
        self.git.error = Some(error);
        self._refresh();
    }

    pub fn open_plan_panel(&mut self) {
        if self.plan.panel_id.is_some() {
            if let Some(panel_id) = self.plan.panel_id {
                if self.tree.focus != panel_id {
                    prepare_panel_focus(self);
                }
                self.tree.focus = panel_id;
            }
            self._refresh();
            return;
        }

        let Some((editor_slot, _index)) = self.tree.find_editor_slot_at_root() else {
            self.set_error("could not open plan panel: editor layout unavailable");
            return;
        };

        prepare_panel_focus(self);
        let plan_id = self.tree.create_plan_panel();
        if !self.tree.replace_root_child(editor_slot, plan_id) {
            self.tree.discard_node(plan_id);
            self.set_error("could not open plan panel: failed to swap editor area");
            return;
        }

        self.plan.panel_id = Some(plan_id);
        self.plan.stashed_editor_root = Some(editor_slot);
        self.plan.scroll = 0;
        self.tree.focus = plan_id;
        self._refresh();
    }

    pub fn close_plan_panel(&mut self) {
        let Some(panel_id) = self.plan.panel_id.take() else {
            return;
        };

        if self.tree.focus == panel_id {
            if let Some(agent_panel) = self.agent.panel_id {
                self.tree.focus = agent_panel;
            } else if let Some(stashed) = self.plan.stashed_editor_root {
                if self.tree.contains(stashed) {
                    let focus = self
                        .tree
                        .views()
                        .map(|(view, _)| view.id)
                        .next()
                        .unwrap_or(stashed);
                    self.tree.focus = focus;
                } else {
                    self.tree.focus = self.tree.prev();
                }
            } else {
                self.tree.focus = self.tree.prev();
            }
        }

        if let Some(stashed) = self.plan.stashed_editor_root.take() {
            let _ = self.tree.replace_root_child(panel_id, stashed);
            self.tree.discard_node(panel_id);
        } else if self.tree.contains(panel_id) {
            self.tree.remove(panel_id);
        }

        self.plan.clear_review();
        self._refresh();
    }

    pub fn focus_plan_panel(&mut self) {
        if let Some(panel_id) = self.plan.panel_id {
            if self.tree.focus != panel_id {
                prepare_panel_focus(self);
            }
            self.tree.focus = panel_id;
        }
    }

    pub fn open_review_panel(&mut self) {
        if let Some(panel_id) = self.review_panel.panel_id {
            if self.tree.focus != panel_id {
                prepare_panel_focus(self);
            }
            self.tree.focus = panel_id;
            return;
        }

        prepare_panel_focus(self);
        let panel_id = self.tree.split_review_panel(Layout::Vertical);
        self.review_panel.panel_id = Some(panel_id);
        self.tree.focus = panel_id;
        self._refresh();
    }

    pub fn close_review_panel(&mut self) {
        let Some(panel_id) = self.review_panel.panel_id.take() else {
            return;
        };
        if self.tree.focus == panel_id {
            self.tree.focus = self.tree.prev();
        }
        if self.tree.contains(panel_id) {
            self.tree.remove(panel_id);
        }
        self._refresh();
    }

    pub fn focus_review_panel(&mut self) {
        if let Some(panel_id) = self.review_panel.panel_id {
            if self.tree.focus != panel_id {
                prepare_panel_focus(self);
            }
            self.tree.focus = panel_id;
        }
    }

    pub fn focus_editor_from_review_panel(&mut self) {
        if self.tree.is_review_panel(self.tree.focus) {
            let focus = self
                .tree
                .views()
                .map(|(view, _)| view.id)
                .next()
                .unwrap_or(self.tree.focus);
            self.tree.focus = focus;
        }
    }

    pub fn focus_editor_from_plan(&mut self) {
        if self.tree.is_plan_panel(self.tree.focus) {
            if let Some(stashed) = self.plan.stashed_editor_root {
                if self.tree.contains(stashed) {
                    let focus = self
                        .tree
                        .views()
                        .map(|(view, _)| view.id)
                        .next()
                        .unwrap_or(stashed);
                    self.tree.focus = focus;
                    return;
                }
            }
            self.tree.focus = self.tree.prev();
        }
    }
}
