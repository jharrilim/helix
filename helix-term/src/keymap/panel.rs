//! Default keymaps for auxiliary panel leaves (git, agent, terminal).

use super::window::{control_w_trie, panel_space_trie};
use super::{KeyTrie, KeyTrieNode};
use crate::commands::MappableCommand;
use crate::{key, keymap};
use helix_core::hashmap;
use helix_view::tree::LeafKind;
use indexmap::IndexMap;
use std::collections::HashMap;

fn shared_panel_prefix() -> KeyTrie {
    let mut map = IndexMap::new();
    map.insert(
        key!(':'),
        KeyTrie::MappableCommand(MappableCommand::command_mode),
    );
    map.insert(key!(' '), panel_space_trie());
    // Merge C-w window prefix keys at top level (same as document normal mode).
    if let KeyTrie::Node(node) = control_w_trie() {
        for (key, trie) in node.map {
            map.insert(key, trie);
        }
    }
    KeyTrie::Node(KeyTrieNode::new("Panel", map))
}

fn with_shared(prefix: KeyTrie) -> KeyTrie {
    let KeyTrie::Node(mut node) = prefix else {
        return prefix;
    };
    if let KeyTrie::Node(shared) = shared_panel_prefix() {
        for (key, trie) in shared.map {
            node.map.insert(key, trie);
        }
    }
    KeyTrie::Node(node)
}

pub fn git_panel() -> KeyTrie {
    with_shared(keymap!({ "Git panel"
        "j" | "down" => git_panel_move_down,
        "k" | "up" => git_panel_move_up,
        "o" | "ret" => git_open,
        "d" => git_diff,
        "a" => git_stage_selected,
        "A" => git_stage_all,
        "c" => git_commit_prompt,
        "r" => git_refresh,
        "esc" => git_focus_editor,
        "q" => git_panel_close,
    }))
}

pub fn agent_panel() -> KeyTrie {
    with_shared(keymap!({ "Agent panel"
        "i" | "a" | "ret" => agent_panel_insert,
        "z" => agent_panel_toggle_collapsible,
        "pageup" => agent_panel_page_up,
        "pagedown" => agent_panel_page_down,
        "q" => agent_panel_close,
    }))
}

pub fn terminal_panel() -> KeyTrie {
    with_shared(keymap!({ "Terminal panel"
        "i" | "a" => terminal_panel_insert,
        "j" => terminal_panel_scroll_down,
        "k" => terminal_panel_scroll_up,
        "g" => terminal_panel_scroll_top,
        "G" => terminal_panel_scroll_bottom,
        "/" => terminal_panel_search,
        "t" => terminal_panel_tab_menu,
        "q" => terminal_panel_close,
    }))
}

pub fn default() -> HashMap<LeafKind, KeyTrie> {
    hashmap! {
        LeafKind::GitPanel => git_panel(),
        LeafKind::AgentPanel => agent_panel(),
        LeafKind::TerminalPanel => terminal_panel(),
    }
}
