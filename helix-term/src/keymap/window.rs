//! Shared window-management key bindings used by document and panel keymaps.

use super::{KeyTrie, KeyTrieNode};
use crate::{ctrl, keymap};
use helix_core::hashmap;
use indexmap::IndexMap;

/// Window navigation bindings (`C-w` prefix and `<space>w` subtree).
///
/// Mirrors the `"Window"` / `"space" -> "w"` sections in [`super::default`].
pub fn window_trie() -> KeyTrie {
    keymap!({ "Window"
        "C-w" | "w" => rotate_view,
        "C-s" | "s" => hsplit,
        "C-v" | "v" => vsplit,
        "C-t" | "t" => transpose_view,
        "f" => goto_file_hsplit,
        "F" => goto_file_vsplit,
        "C-q" | "q" => wclose,
        "C-o" | "o" => wonly,
        "C-h" | "h" | "left" => jump_view_left,
        "C-j" | "j" | "down" => jump_view_down,
        "C-k" | "k" | "up" => jump_view_up,
        "C-l" | "l" | "right" => jump_view_right,
        "H" => swap_view_left,
        "J" => swap_view_down,
        "K" => swap_view_up,
        "L" => swap_view_right,
        "n" => { "New split scratch buffer"
            "C-s" | "s" => hsplit_new,
            "C-v" | "v" => vsplit_new,
        },
    })
}

/// Top-level `C-w` prefix wrapping [`window_trie`].
pub fn control_w_trie() -> KeyTrie {
    let mut map = IndexMap::new();
    map.insert(ctrl!('w'), window_trie());
    KeyTrie::Node(KeyTrieNode::new("Window", map))
}

/// `<space>` prefix shared by auxiliary panels: window nav + panel switching.
pub fn panel_space_trie() -> KeyTrie {
    keymap!({ "Space"
        "w" => { "Window"
            "C-w" | "w" => rotate_view,
            "C-s" | "s" => hsplit,
            "C-v" | "v" => vsplit,
            "C-t" | "t" => transpose_view,
            "f" => goto_file_hsplit,
            "F" => goto_file_vsplit,
            "C-q" | "q" => wclose,
            "C-o" | "o" => wonly,
            "C-h" | "h" | "left" => jump_view_left,
            "C-j" | "j" | "down" => jump_view_down,
            "C-k" | "k" | "up" => jump_view_up,
            "C-l" | "l" | "right" => jump_view_right,
            "H" => swap_view_left,
            "J" => swap_view_down,
            "K" => swap_view_up,
            "L" => swap_view_right,
            "n" => { "New split scratch buffer"
                "C-s" | "s" => hsplit_new,
                "C-v" | "v" => vsplit_new,
            },
        },
        "V" => git_panel_toggle,
        "A" => { "Agent"
            "a" => agent_open,
            "A" => agent_focus,
            "c" => agent_close,
            "h" => agent_history,
            "n" => agent_new,
            "s" => agent_send,
            "S" => agent_stop,
            "C" => agent_clear,
            "m" => agent_mode,
        },
        "t" => { "Terminal"
            "o" => terminal_open,
            "c" => terminal_close,
            "i" => terminal_insert_mode,
            "s" => terminal_send,
            "n" => terminal_new,
            "l" => terminal_list,
        },
    })
}
