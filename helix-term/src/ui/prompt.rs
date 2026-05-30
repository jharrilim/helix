use crate::compositor::{Component, Compositor, Context, Event, EventResult};
use crate::{alt, ctrl, key, shift, ui};
use arc_swap::ArcSwap;
use helix_core::syntax;
use helix_view::document::Mode;
use helix_view::input::KeyEvent;
use helix_view::keyboard::KeyCode;
use std::sync::Arc;
use std::{borrow::Cow, ops::RangeFrom};
use tui::buffer::Buffer as Surface;
use tui::text::Span;
use tui::widgets::{Block, Widget};

use helix_core::{
    unicode::segmentation::{GraphemeCursor, UnicodeSegmentation},
    unicode::width::UnicodeWidthStr,
    Position,
};
use helix_view::{
    graphics::{CursorKind, Margin, Modifier, Rect},
    Editor,
};

type PromptCharHandler = Box<dyn Fn(&mut Prompt, char, &Context)>;

pub type Completion = (RangeFrom<usize>, Span<'static>);
type CompletionFn = Box<dyn FnMut(&Editor, &str) -> Vec<Completion>>;
type CallbackFn = Box<dyn FnMut(&mut Context, &str, PromptEvent)>;
pub type DocFn = Box<dyn Fn(&str) -> Option<Cow<str>>>;

pub struct Prompt {
    prompt: Cow<'static, str>,
    line: String,
    cursor: usize,
    // Fields used for Component callbacks and rendering:
    line_area: Rect,
    anchor: usize,
    truncate_start: bool,
    truncate_end: bool,
    // ---
    completion: Vec<Completion>,
    selection: Option<usize>,
    history_register: Option<char>,
    history_pos: Option<usize>,
    completion_fn: CompletionFn,
    callback_fn: CallbackFn,
    pub doc_fn: DocFn,
    next_char_handler: Option<PromptCharHandler>,
    language: Option<(&'static str, Arc<ArcSwap<syntax::Loader>>)>,
    /// When true, clear the editor area above the prompt line each frame.
    clear_above: bool,
    /// When true, Enter submits and Shift+Enter inserts a newline.
    multiline: bool,
    /// When true, Enter invokes [`PromptEvent::Advance`] instead of [`PromptEvent::Validate`].
    submit_to_footer_buttons: bool,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum PromptEvent {
    /// The prompt input has been updated.
    Update,
    /// Validate and finalize the change.
    Validate,
    /// Abort the change, reverting to the initial state.
    Abort,
    /// Move to a follow-up UI step (e.g. footer buttons) without validating yet.
    Advance,
}

pub enum CompletionDirection {
    Forward,
    Backward,
}

#[derive(Debug, Clone, Copy)]
pub enum Movement {
    BackwardChar(usize),
    BackwardWord(usize),
    ForwardChar(usize),
    ForwardWord(usize),
    StartOfLine,
    EndOfLine,
    None,
}

fn is_word_sep(c: char) -> bool {
    c == std::path::MAIN_SEPARATOR || c.is_whitespace()
}

impl Prompt {
    pub fn new(
        prompt: Cow<'static, str>,
        history_register: Option<char>,
        completion_fn: impl FnMut(&Editor, &str) -> Vec<Completion> + 'static,
        callback_fn: impl FnMut(&mut Context, &str, PromptEvent) + 'static,
    ) -> Self {
        Self {
            prompt,
            line: String::new(),
            cursor: 0,
            line_area: Rect::default(),
            anchor: 0,
            truncate_start: false,
            truncate_end: false,
            completion: Vec::new(),
            selection: None,
            history_register,
            history_pos: None,
            completion_fn: Box::new(completion_fn),
            callback_fn: Box::new(callback_fn),
            doc_fn: Box::new(|_| None),
            next_char_handler: None,
            language: None,
            clear_above: true,
            multiline: false,
            submit_to_footer_buttons: false,
        }
    }

    /// Keep the editor visible behind this prompt (for contextual prompts like review comments).
    pub fn retain_editor(mut self) -> Self {
        self.clear_above = false;
        self
    }

    /// Allow multiline input: Shift+Enter inserts newline, Enter submits.
    pub fn multiline(mut self) -> Self {
        self.multiline = true;
        self
    }

    /// Enter moves to footer buttons; use [`PromptEvent::Advance`] in the callback.
    pub fn submit_to_footer_buttons(mut self) -> Self {
        self.submit_to_footer_buttons = true;
        self
    }

    /// Byte range of the logical line containing the cursor.
    fn logical_line_range(&self) -> (usize, usize) {
        let start = self.line[..self.cursor]
            .rfind('\n')
            .map(|i| i + 1)
            .unwrap_or(0);
        let end = self.line[start..]
            .find('\n')
            .map(|i| start + i)
            .unwrap_or(self.line.len());
        (start, end)
    }

    fn cursor_in_display_line(&self) -> usize {
        let (start, _) = self.logical_line_range();
        self.cursor - start
    }

    fn logical_line_index(&self) -> usize {
        self.line[..self.cursor].bytes().filter(|b| *b == b'\n').count()
    }

    fn logical_line_count(&self) -> usize {
        self.line.bytes().filter(|b| *b == b'\n').count() + 1
    }

    fn move_logical_line(&mut self, delta: isize) {
        let lines: Vec<&str> = self.line.split('\n').collect();
        if lines.is_empty() {
            return;
        }
        let col = self.cursor_in_display_line();
        let idx = (self.logical_line_index() as isize + delta)
            .clamp(0, lines.len() as isize - 1) as usize;
        let mut offset = 0usize;
        for (i, line) in lines.iter().enumerate() {
            if i == idx {
                self.cursor = offset + col.min(line.len());
                return;
            }
            offset += line.len() + 1;
        }
    }

    /// Gets the byte index in the input representing the current cursor location.
    #[inline]
    pub(crate) fn position(&self) -> usize {
        self.cursor
    }

    pub fn with_line(mut self, line: String, editor: &Editor) -> Self {
        self.set_line(line, editor);
        self
    }

    pub fn set_line(&mut self, line: String, editor: &Editor) {
        let cursor = line.len();
        self.line = line;
        self.cursor = cursor;
        self.recalculate_completion(editor);
    }

    pub fn with_language(
        mut self,
        language: &'static str,
        loader: Arc<ArcSwap<syntax::Loader>>,
    ) -> Self {
        self.language = Some((language, loader));
        self
    }

    pub fn line(&self) -> &String {
        &self.line
    }

    pub fn with_history_register(&mut self, history_register: Option<char>) -> &mut Self {
        self.history_register = history_register;
        self
    }

    pub(crate) fn history_register(&self) -> Option<char> {
        self.history_register
    }

    pub(crate) fn first_history_completion<'a>(
        &'a self,
        editor: &'a Editor,
    ) -> Option<Cow<'a, str>> {
        self.history_register
            .and_then(|reg| editor.registers.first(reg, editor))
    }

    pub fn recalculate_completion(&mut self, editor: &Editor) {
        self.exit_selection();
        self.completion = (self.completion_fn)(editor, &self.line);
    }

    /// Compute the cursor position after applying movement
    /// Taken from: <https://github.com/wez/wezterm/blob/e0b62d07ca9bf8ce69a61e30a3c20e7abc48ce7e/termwiz/src/lineedit/mod.rs#L516-L611>
    fn eval_movement(&self, movement: Movement) -> usize {
        if self.multiline {
            return self.eval_multiline_movement(movement);
        }
        match movement {
            Movement::BackwardChar(rep) => {
                let mut position = self.cursor;
                for _ in 0..rep {
                    let mut cursor = GraphemeCursor::new(position, self.line.len(), false);
                    if let Ok(Some(pos)) = cursor.prev_boundary(&self.line, 0) {
                        position = pos;
                    } else {
                        break;
                    }
                }
                position
            }
            Movement::BackwardWord(rep) => {
                let char_indices: Vec<(usize, char)> = self.line.char_indices().collect();
                if char_indices.is_empty() {
                    return self.cursor;
                }
                let mut char_position = char_indices
                    .iter()
                    .position(|(idx, _)| *idx == self.cursor)
                    .unwrap_or(char_indices.len() - 1);

                for _ in 0..rep {
                    if char_position == 0 {
                        break;
                    }

                    let mut found = None;
                    for prev in (0..char_position - 1).rev() {
                        if is_word_sep(char_indices[prev].1) {
                            found = Some(prev + 1);
                            break;
                        }
                    }

                    char_position = found.unwrap_or(0);
                }
                char_indices[char_position].0
            }
            Movement::ForwardWord(rep) => {
                let char_indices: Vec<(usize, char)> = self.line.char_indices().collect();
                if char_indices.is_empty() {
                    return self.cursor;
                }
                let mut char_position = char_indices
                    .iter()
                    .position(|(idx, _)| *idx == self.cursor)
                    .unwrap_or(char_indices.len());

                for _ in 0..rep {
                    // Skip any non-whitespace characters
                    while char_position < char_indices.len()
                        && !is_word_sep(char_indices[char_position].1)
                    {
                        char_position += 1;
                    }

                    // Skip any whitespace characters
                    while char_position < char_indices.len()
                        && is_word_sep(char_indices[char_position].1)
                    {
                        char_position += 1;
                    }

                    // We are now on the start of the next word
                }
                char_indices
                    .get(char_position)
                    .map(|(i, _)| *i)
                    .unwrap_or_else(|| self.line.len())
            }
            Movement::ForwardChar(rep) => {
                let mut position = self.cursor;
                for _ in 0..rep {
                    let mut cursor = GraphemeCursor::new(position, self.line.len(), false);
                    if let Ok(Some(pos)) = cursor.next_boundary(&self.line, 0) {
                        position = pos;
                    } else {
                        break;
                    }
                }
                position
            }
            Movement::StartOfLine => 0,
            Movement::EndOfLine => self.line.len(),
            Movement::None => self.cursor,
        }
    }

    fn eval_multiline_movement(&self, movement: Movement) -> usize {
        let (line_start, line_end) = self.logical_line_range();
        match movement {
            Movement::StartOfLine => line_start,
            Movement::EndOfLine => line_end,
            Movement::BackwardChar(rep) => {
                let mut position = self.cursor;
                for _ in 0..rep {
                    if position <= line_start {
                        break;
                    }
                    let mut cursor = GraphemeCursor::new(position, self.line.len(), false);
                    if let Ok(Some(pos)) = cursor.prev_boundary(&self.line, 0) {
                        position = pos;
                    } else {
                        break;
                    }
                }
                position
            }
            Movement::ForwardChar(rep) => {
                let mut position = self.cursor;
                for _ in 0..rep {
                    if position >= line_end {
                        break;
                    }
                    let mut cursor = GraphemeCursor::new(position, self.line.len(), false);
                    if let Ok(Some(pos)) = cursor.next_boundary(&self.line, 0) {
                        position = pos;
                    } else {
                        break;
                    }
                }
                position
            }
            Movement::BackwardWord(rep) => {
                self.eval_multiline_movement(Movement::BackwardChar(rep))
            }
            Movement::ForwardWord(rep) => {
                self.eval_multiline_movement(Movement::ForwardChar(rep))
            }
            Movement::None => self.cursor,
        }
    }

    pub fn insert_char(&mut self, c: char, cx: &Context) {
        if let Some(handler) = &self.next_char_handler.take() {
            handler(self, c, cx);

            self.next_char_handler = None;
            return;
        }

        self.line.insert(self.cursor, c);
        let mut cursor = GraphemeCursor::new(self.cursor, self.line.len(), false);
        if let Ok(Some(pos)) = cursor.next_boundary(&self.line, 0) {
            self.cursor = pos;
        }
        self.recalculate_completion(cx.editor);
    }

    pub fn insert_str(&mut self, s: &str, editor: &Editor) {
        self.line.insert_str(self.cursor, s);
        self.cursor += s.len();
        self.recalculate_completion(editor);
    }

    pub fn move_cursor(&mut self, movement: Movement) {
        let pos = self.eval_movement(movement);
        self.cursor = pos
    }

    pub fn move_start(&mut self) {
        if self.multiline {
            let (start, _) = self.logical_line_range();
            self.cursor = start;
        } else {
            self.cursor = 0;
        }
    }

    pub fn move_end(&mut self) {
        if self.multiline {
            let (_, end) = self.logical_line_range();
            self.cursor = end;
        } else {
            self.cursor = self.line.len();
        }
    }

    pub fn delete_char_backwards(&mut self, editor: &Editor) {
        if self.multiline {
            let (line_start, _) = self.logical_line_range();
            if self.cursor == line_start && line_start > 0 {
                self.line.replace_range(line_start - 1..line_start, "");
                self.cursor = line_start - 1;
                self.recalculate_completion(editor);
                return;
            }
        }
        let pos = self.eval_movement(Movement::BackwardChar(1));
        self.line.replace_range(pos..self.cursor, "");
        self.cursor = pos;

        self.recalculate_completion(editor);
    }

    pub fn delete_char_forwards(&mut self, editor: &Editor) {
        let pos = self.eval_movement(Movement::ForwardChar(1));
        self.line.replace_range(self.cursor..pos, "");

        self.recalculate_completion(editor);
    }

    pub fn delete_word_backwards(&mut self, editor: &Editor) {
        let pos = self.eval_movement(Movement::BackwardWord(1));
        self.line.replace_range(pos..self.cursor, "");
        self.cursor = pos;

        self.recalculate_completion(editor);
    }

    pub fn delete_word_forwards(&mut self, editor: &Editor) {
        let pos = self.eval_movement(Movement::ForwardWord(1));
        self.line.replace_range(self.cursor..pos, "");

        self.recalculate_completion(editor);
    }

    pub fn kill_to_start_of_line(&mut self, editor: &Editor) {
        let pos = self.eval_movement(Movement::StartOfLine);
        self.line.replace_range(pos..self.cursor, "");
        self.cursor = pos;

        self.recalculate_completion(editor);
    }

    pub fn kill_to_end_of_line(&mut self, editor: &Editor) {
        let pos = self.eval_movement(Movement::EndOfLine);
        self.line.replace_range(self.cursor..pos, "");

        self.recalculate_completion(editor);
    }

    pub fn clear(&mut self, editor: &Editor) {
        self.line.clear();
        self.cursor = 0;
        self.recalculate_completion(editor);
    }

    pub fn change_history(
        &mut self,
        cx: &mut Context,
        register: char,
        direction: CompletionDirection,
    ) {
        (self.callback_fn)(cx, &self.line, PromptEvent::Abort);
        let mut values = match cx.editor.registers.read(register, cx.editor) {
            Some(values) if values.len() > 0 => values.rev(),
            _ => return,
        };

        let end = values.len().saturating_sub(1);

        let index = match direction {
            CompletionDirection::Forward => self.history_pos.map_or(0, |i| i + 1),
            CompletionDirection::Backward => self
                .history_pos
                .unwrap_or_else(|| values.len())
                .saturating_sub(1),
        }
        .min(end);

        self.line = values.nth(index).unwrap().to_string();
        // Appease the borrow checker.
        drop(values);

        self.history_pos = Some(index);

        self.move_end();
        (self.callback_fn)(cx, &self.line, PromptEvent::Update);
        self.recalculate_completion(cx.editor);
    }

    pub fn change_completion_selection(&mut self, direction: CompletionDirection) {
        if self.completion.is_empty() {
            return;
        }

        let index = match direction {
            CompletionDirection::Forward => self.selection.map_or(0, |i| i + 1),
            CompletionDirection::Backward => {
                self.selection.unwrap_or(0) + self.completion.len() - 1
            }
        } % self.completion.len();

        self.selection = Some(index);

        let (range, item) = &self.completion[index];

        self.line.replace_range(range.clone(), &item.content);

        self.move_end();
    }

    pub fn exit_selection(&mut self) {
        self.selection = None;
    }
}

const BASE_WIDTH: u16 = 30;

impl Prompt {
    pub fn render_prompt(&mut self, area: Rect, surface: &mut Surface, cx: &mut Context) {
        let theme = &cx.editor.theme;
        let prompt_color = if self.clear_above {
            theme.get("ui.text")
        } else {
            theme
                .try_get("ui.text.info")
                .unwrap_or_else(|| theme.get("ui.text"))
                .add_modifier(Modifier::BOLD)
        };
        let completion_color = theme.get("ui.menu");
        let selected_color = theme.get("ui.menu.selected");
        let suggestion_color = theme.get("ui.text.inactive");
        let background = theme.get("ui.background");
        // completion

        let max_len = self
            .completion
            .iter()
            .map(|(_, completion)| completion.content.len() as u16)
            .max()
            .unwrap_or(BASE_WIDTH)
            .max(BASE_WIDTH);

        let cols = std::cmp::max(1, area.width / max_len);
        let col_width = (area.width.saturating_sub(cols)) / cols;

        let height = (self.completion.len() as u16)
            .div_ceil(cols)
            .min(10) // at most 10 rows (or less)
            .min(area.height.saturating_sub(1));

        let completion_area = Rect::new(
            area.x,
            (area.height - height).saturating_sub(1),
            area.width,
            height,
        );

        if completion_area.height > 0 && !self.completion.is_empty() {
            let area = completion_area;
            let background = theme.get("ui.menu");

            let items = height as usize * cols as usize;

            let offset = self
                .selection
                .map(|selection| selection / items * items)
                .unwrap_or_default();

            surface.clear_with(area, background);

            let mut row = 0;
            let mut col = 0;

            for (i, (_range, completion)) in
                self.completion.iter().enumerate().skip(offset).take(items)
            {
                let is_selected = Some(i) == self.selection;

                let completion_item_style = if is_selected {
                    selected_color
                } else {
                    completion_color.patch(completion.style)
                };

                surface.set_stringn(
                    area.x + col * (1 + col_width),
                    area.y + row,
                    &completion.content,
                    col_width.saturating_sub(1) as usize,
                    completion_item_style,
                );

                row += 1;
                if row > area.height - 1 {
                    row = 0;
                    col += 1;
                }
            }
        }

        if let Some(doc) = (self.doc_fn)(&self.line) {
            let mut text = ui::Text::new(doc.to_string());

            let max_width = BASE_WIDTH * 3;
            let horizontal_padding = 2; // border + margin
            let vertical_padding = 1; // border only
            let text_width = max_width - horizontal_padding * 2;

            let viewport = area;

            let (_width, height) = ui::text::required_size(&text.contents, text_width);

            let area = viewport.intersection(Rect::new(
                completion_area.x,
                completion_area
                    .y
                    .saturating_sub(height + vertical_padding * 2),
                max_width,
                height + vertical_padding * 2,
            ));

            let background = theme.get("ui.help");
            surface.clear_with(area, background);

            let block = Block::bordered()
                // .title(self.title.as_str())
                .border_style(background);

            let inner = block.inner(area).inner(Margin::horizontal(1));

            block.render(area, surface);
            text.render(inner, surface, cx);
        }

        let line = area.height - 1;
        if self.clear_above {
            surface.clear_with(area.clip_top(line), background);
        } else {
            surface.clear_with(
                Rect::new(area.x, area.y + line, area.width, 1),
                background,
            );
        }
        // render buffer text
        let mut prompt_label = self.prompt.to_string();
        if self.multiline && self.line.contains('\n') {
            let _ = std::fmt::Write::write_fmt(
                &mut prompt_label,
                format_args!(
                    " ({}/{})",
                    self.logical_line_index() + 1,
                    self.logical_line_count()
                ),
            );
        }
        surface.set_string(area.x, area.y + line, &prompt_label, prompt_color);

        self.line_area = area
            .clip_left(prompt_label.len() as u16)
            .clip_top(line)
            .clip_right(2);

        let (line_start, render_line) = if self.multiline {
            let (start, end) = self.logical_line_range();
            (start, self.line[start..end].to_string())
        } else {
            (0, self.line.clone())
        };
        let cursor_in_line = self.cursor.saturating_sub(line_start);

        if render_line.is_empty() {
            self.anchor = 0;
            // Show the most recently entered value as a suggestion.
            if let Some(suggestion) = self.first_history_completion(cx.editor) {
                surface.set_string(
                    self.line_area.x,
                    self.line_area.y,
                    suggestion,
                    suggestion_color,
                );
            }
        } else if let Some((language, loader)) = self.language.as_ref() {
            let mut text: ui::text::Text = crate::ui::markdown::highlighted_code_block(
                &render_line,
                language,
                Some(&cx.editor.theme),
                &loader.load(),
                None,
            )
            .into();
            text.render(self.line_area, surface, cx);
        } else {
            let line_width = self.line_area.width as usize;

            if render_line.width() < line_width {
                self.anchor = 0;
            } else if cursor_in_line <= self.anchor {
                self.anchor = render_line[..cursor_in_line]
                    .grapheme_indices(true)
                    .next_back()
                    .map(|(i, _)| i)
                    .unwrap_or_default();
            } else if render_line[self.anchor..cursor_in_line].width() > line_width {
                let mut width = 0;
                self.anchor = render_line[..cursor_in_line]
                    .grapheme_indices(true)
                    .rev()
                    .find_map(|(idx, g)| {
                        width += g.width();
                        if width > line_width {
                            Some(idx + g.len())
                        } else {
                            None
                        }
                    })
                    .unwrap();
            }

            self.truncate_start = self.anchor > 0;
            self.truncate_end = render_line[self.anchor..].width() > line_width;

            if self.truncate_end
                && render_line[self.anchor..cursor_in_line].width() >= line_width
            {
                self.anchor += render_line[self.anchor..]
                    .grapheme_indices(true)
                    .find_map(|(idx, g)| {
                        if g.width() > 0 {
                            Some(idx + g.len())
                        } else {
                            None
                        }
                    })
                    .unwrap();
            }

            surface.set_string_anchored(
                self.line_area.x,
                self.line_area.y,
                self.truncate_start,
                self.truncate_end,
                &render_line[self.anchor..],
                line_width,
                |_| prompt_color,
            );
        }

        self.highlight_input_cursor(surface, &render_line, line_start, prompt_color);
    }

    fn highlight_input_cursor(
        &self,
        surface: &mut Surface,
        render_line: &str,
        line_start: usize,
        style: helix_view::theme::Style,
    ) {
        if self.line_area.width == 0 {
            return;
        }
        let cursor_in_line = self.cursor.saturating_sub(line_start);
        let cursor_x = self.line_area.x.saturating_add(
            render_line[..cursor_in_line.min(render_line.len())]
                .graphemes(true)
                .map(|g| g.width())
                .sum::<usize>() as u16,
        );
        let cursor_y = self.line_area.y;
        if let Some(cell) = surface.get_mut(
            cursor_x.min(self.line_area.right().saturating_sub(1)),
            cursor_y,
        ) {
            cell.set_style(style.add_modifier(Modifier::REVERSED));
        }
    }
}

impl Component for Prompt {
    fn handle_event(&mut self, event: &Event, cx: &mut Context) -> EventResult {
        let event = match event {
            Event::Paste(data) => {
                self.insert_str(data, cx.editor);
                self.recalculate_completion(cx.editor);
                (self.callback_fn)(cx, &self.line, PromptEvent::Update);
                return EventResult::Consumed(None);
            }
            Event::Key(event) => *event,
            Event::Resize(..) => return EventResult::Consumed(None),
            _ => return EventResult::Ignored(None),
        };

        let close_fn = EventResult::Consumed(Some(Box::new(|compositor: &mut Compositor, _| {
            // remove the layer
            compositor.pop();
        })));

        match event {
            ctrl!('c') | key!(Esc) => {
                (self.callback_fn)(cx, &self.line, PromptEvent::Abort);
                return close_fn;
            }
            alt!('b') | ctrl!(Left) => self.move_cursor(Movement::BackwardWord(1)),
            alt!('f') | ctrl!(Right) => self.move_cursor(Movement::ForwardWord(1)),
            ctrl!('b') | key!(Left) => self.move_cursor(Movement::BackwardChar(1)),
            ctrl!('f') | key!(Right) => self.move_cursor(Movement::ForwardChar(1)),
            ctrl!('e') | key!(End) => self.move_end(),
            ctrl!('a') | key!(Home) => self.move_start(),
            ctrl!('w') | alt!(Backspace) | ctrl!(Backspace) => {
                self.delete_word_backwards(cx.editor);
                (self.callback_fn)(cx, &self.line, PromptEvent::Update);
            }
            alt!('d') | alt!(Delete) | ctrl!(Delete) => {
                self.delete_word_forwards(cx.editor);
                (self.callback_fn)(cx, &self.line, PromptEvent::Update);
            }
            ctrl!('k') => {
                self.kill_to_end_of_line(cx.editor);
                (self.callback_fn)(cx, &self.line, PromptEvent::Update);
            }
            ctrl!('u') => {
                self.kill_to_start_of_line(cx.editor);
                (self.callback_fn)(cx, &self.line, PromptEvent::Update);
            }
            ctrl!('h') | key!(Backspace) | shift!(Backspace) => {
                self.delete_char_backwards(cx.editor);
                (self.callback_fn)(cx, &self.line, PromptEvent::Update);
            }
            ctrl!('d') | key!(Delete) => {
                self.delete_char_forwards(cx.editor);
                (self.callback_fn)(cx, &self.line, PromptEvent::Update);
            }
            shift!(Enter) if self.multiline => {
                self.insert_char('\n', cx);
                (self.callback_fn)(cx, &self.line, PromptEvent::Update);
            }
            ctrl!('s') => {
                if cx.editor.is_document_view_focused() {
                    let (view, doc) = current!(cx.editor);
                    let text = doc.text().slice(..);

                    use helix_core::textobject;
                    let range = textobject::textobject_word(
                        text,
                        doc.selection(view.id).primary(),
                        textobject::TextObject::Inside,
                        1,
                        false,
                    );
                    let line = text.slice(range.from()..range.to()).to_string();
                    if !line.is_empty() {
                        self.insert_str(line.as_str(), cx.editor);
                        (self.callback_fn)(cx, &self.line, PromptEvent::Update);
                    }
                }
            }
            key!(Enter) => {
                if self.selection.is_some() && self.line.ends_with(std::path::MAIN_SEPARATOR) {
                    self.recalculate_completion(cx.editor);
                } else {
                    let last_item = self
                        .first_history_completion(cx.editor)
                        .map(|entry| entry.to_string())
                        .unwrap_or_else(|| String::from(""));

                    // handle executing with last command in history if nothing entered
                    let input = if self.line.is_empty() {
                        &last_item
                    } else {
                        if last_item != self.line {
                            // store in history
                            if let Some(register) = self.history_register {
                                if let Err(err) =
                                    cx.editor.registers.push(register, self.line.clone())
                                {
                                    cx.editor.set_error(err.to_string());
                                }
                            };
                        }

                        &self.line
                    };

                    if self.submit_to_footer_buttons {
                        (self.callback_fn)(cx, input, PromptEvent::Advance);
                    } else {
                        (self.callback_fn)(cx, input, PromptEvent::Validate);
                    }

                    return close_fn;
                }
            }
            ctrl!('p') | key!(Up) => {
                if self.multiline && self.history_register.is_none() {
                    self.move_logical_line(-1);
                } else if let Some(register) = self.history_register {
                    self.change_history(cx, register, CompletionDirection::Backward);
                }
            }
            ctrl!('n') | key!(Down) => {
                if self.multiline && self.history_register.is_none() {
                    self.move_logical_line(1);
                } else if let Some(register) = self.history_register {
                    self.change_history(cx, register, CompletionDirection::Forward);
                }
            }
            key!(Tab) => {
                self.change_completion_selection(CompletionDirection::Forward);
                // if single completion candidate is a directory list content in completion
                if self.completion.len() == 1 && self.line.ends_with(std::path::MAIN_SEPARATOR) {
                    self.recalculate_completion(cx.editor);
                }
                (self.callback_fn)(cx, &self.line, PromptEvent::Update)
            }
            shift!(Tab) => {
                self.change_completion_selection(CompletionDirection::Backward);
                (self.callback_fn)(cx, &self.line, PromptEvent::Update)
            }
            ctrl!('q') => self.exit_selection(),
            ctrl!('r') => {
                self.completion = cx
                    .editor
                    .registers
                    .iter_preview()
                    .map(|(ch, preview)| (0.., format!("{} {}", ch, &preview).into()))
                    .collect();
                self.next_char_handler = Some(Box::new(|prompt, c, context| {
                    prompt.insert_str(
                        &context
                            .editor
                            .registers
                            .first(c, context.editor)
                            .unwrap_or_default(),
                        context.editor,
                    );
                }));
                (self.callback_fn)(cx, &self.line, PromptEvent::Update);
                return EventResult::Consumed(None);
            }
            // any char event that's not mapped to any other combo
            KeyEvent {
                code: KeyCode::Char(c),
                modifiers: _,
            } => {
                self.insert_char(c, cx);
                (self.callback_fn)(cx, &self.line, PromptEvent::Update);
            }
            _ => (),
        };

        EventResult::Consumed(None)
    }

    fn render(&mut self, area: Rect, surface: &mut Surface, cx: &mut Context) {
        self.render_prompt(area, surface, cx)
    }

    fn cursor(&self, area: Rect, editor: &Editor) -> (Option<Position>, CursorKind) {
        let prompt_len = if self.multiline && self.line.contains('\n') {
            self.prompt.len()
                + format!(
                    " ({}/{})",
                    self.logical_line_index() + 1,
                    self.logical_line_count()
                )
                .len()
        } else {
            self.prompt.len()
        };
        let area = area
            .clip_left(prompt_len as u16)
            .clip_right(if self.prompt.is_empty() { 2 } else { 0 });

        let (line_start, line_end) = if self.multiline {
            self.logical_line_range()
        } else {
            (0, self.line.len())
        };
        let render_slice = &self.line[line_start..line_end];
        let cursor_in_line = self.cursor.saturating_sub(line_start);
        let mut col = area.left() as usize + render_slice[self.anchor..cursor_in_line].width();

        if self.truncate_end
            && render_slice[self.anchor..cursor_in_line].width() >= self.line_area.width as usize
        {
            col -= 1;
        }

        if self.truncate_start && cursor_in_line == self.anchor {
            col += render_slice[cursor_in_line..]
                .graphemes(true)
                .next()
                .map_or(0, |g| g.width());
        }

        let line = area.height as usize - 1;

        let kind = if self.clear_above {
            editor.config().cursor_shape.from_mode(Mode::Insert)
        } else {
            // Cursor is drawn with reversed style in `highlight_input_cursor`.
            CursorKind::Hidden
        };

        (
            Some(Position::new(area.y as usize + line, col)),
            kind,
        )
    }
}
