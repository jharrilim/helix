use helix_pty::TerminalSession;

#[test]
fn sgr_color_renders_text() {
    let mut session = TerminalSession::new(24, 80, 1000);
    session.feed(b"\x1b[31mred\x1b[0m");
    assert_eq!(session.line(0), "red");
}

#[test]
fn cursor_motion_places_text() {
    let mut session = TerminalSession::new(24, 80, 1000);
    session.feed(b"\x1b[2;5HX");
    assert_eq!(session.line(1), "    X");
}

#[test]
fn clear_line_clears_content() {
    let mut session = TerminalSession::new(24, 80, 1000);
    session.feed(b"hello\x1b[2K");
    assert_eq!(session.line(0), "");
}

#[test]
fn scrollback_accumulates_history() {
    let mut session = TerminalSession::new(5, 10, 1000);
    for i in 0..10 {
        session.feed(format!("line{i}\n").as_bytes());
    }
    assert!(session.scrollback_len() > 0);
}

#[test]
fn alternate_screen_toggles_without_panic() {
    let mut session = TerminalSession::new(24, 80, 1000);
    session.feed(b"\x1b[?1049h");
    session.feed(b"alt screen");
    session.feed(b"\x1b[?1049l");
    let _ = session.line(0);
}

#[test]
fn resize_updates_dimensions() {
    let mut session = TerminalSession::new(24, 80, 1000);
    session.resize(10, 40);
    assert_eq!(session.rows(), 10);
    assert_eq!(session.cols(), 40);
}
