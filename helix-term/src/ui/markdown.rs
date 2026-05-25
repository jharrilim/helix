use crate::compositor::{Component, Context};
use arc_swap::ArcSwap;
use tui::{
    buffer::Buffer as Surface,
    text::{Span, Spans, Text},
};

use std::sync::Arc;

use pulldown_cmark::{CodeBlockKind, Event, HeadingLevel, Options, Parser, Tag, TagEnd};

use helix_core::{
    syntax::{self, HighlightEvent, OverlayHighlights},
    RopeSlice, Syntax,
};
use helix_view::{
    graphics::{Margin, Rect, Style},
    theme::Modifier,
    Theme,
};

fn styled_multiline_text<'a>(text: &str, style: Style) -> Text<'a> {
    let spans: Vec<_> = text
        .lines()
        .map(|line| Span::styled(line.to_string(), style))
        .map(Spans::from)
        .collect();
    Text::from(spans)
}

pub fn highlighted_code_block<'a>(
    text: &str,
    language: &str,
    theme: Option<&Theme>,
    loader: &syntax::Loader,
    // Optional overlay highlights to mix in with the syntax highlights.
    //
    // Note that `OverlayHighlights` is typically used with char indexing but the only caller
    // which passes this parameter currently passes **byte indices** instead.
    additional_highlight_spans: Option<OverlayHighlights>,
) -> Text<'a> {
    let mut spans = Vec::new();
    let mut lines = Vec::new();

    let get_theme = |key: &str| -> Style { theme.map(|t| t.get(key)).unwrap_or_default() };
    let text_style = get_theme(Markdown::TEXT_STYLE);
    let code_style = get_theme(Markdown::BLOCK_STYLE);

    let theme = match theme {
        Some(t) => t,
        None => return styled_multiline_text(text, code_style),
    };

    let ropeslice = RopeSlice::from(text);
    let Some(syntax) = loader
        .language_for_match(RopeSlice::from(language))
        .and_then(|lang| Syntax::new(ropeslice, lang, loader).ok())
    else {
        return styled_multiline_text(text, code_style);
    };

    let mut syntax_highlighter = syntax.highlighter(ropeslice, loader, ..);
    let mut syntax_highlight_stack = Vec::new();
    let mut overlay_highlight_stack = Vec::new();
    let mut overlay_highlighter = syntax::OverlayHighlighter::new(additional_highlight_spans);
    let mut pos = 0;

    while pos < ropeslice.len_bytes() as u32 {
        if pos == syntax_highlighter.next_event_offset() {
            let (event, new_highlights) = syntax_highlighter.advance();
            if event == HighlightEvent::Refresh {
                syntax_highlight_stack.clear();
            }
            syntax_highlight_stack.extend(new_highlights);
        } else if pos == overlay_highlighter.next_event_offset() as u32 {
            let (event, new_highlights) = overlay_highlighter.advance();
            if event == HighlightEvent::Refresh {
                overlay_highlight_stack.clear();
            }
            overlay_highlight_stack.extend(new_highlights)
        }

        let start = pos;
        pos = syntax_highlighter
            .next_event_offset()
            .min(overlay_highlighter.next_event_offset() as u32);
        if pos == u32::MAX {
            pos = ropeslice.len_bytes() as u32;
        }
        if pos == start {
            continue;
        }
        // The highlighter should always move forward.
        // If the highlighter malfunctions, bail on syntax highlighting and log an error.
        debug_assert!(pos > start);
        if pos < start {
            log::error!("Failed to highlight '{language}': {text:?}");
            return styled_multiline_text(text, code_style);
        }

        let style = syntax_highlight_stack
            .iter()
            .chain(overlay_highlight_stack.iter())
            .fold(text_style, |acc, highlight| {
                acc.patch(theme.highlight(*highlight))
            });

        let mut slice = &text[start as usize..pos as usize];
        // TODO: do we need to handle all unicode line endings
        // here, or is just '\n' okay?
        while let Some(end) = slice.find('\n') {
            // emit span up to newline
            let text = &slice[..end];
            let text = text.replace('\t', "    "); // replace tabs
            let span = Span::styled(text, style);
            spans.push(span);

            // truncate slice to after newline
            slice = &slice[end + 1..];

            // make a new line
            let spans = std::mem::take(&mut spans);
            lines.push(Spans::from(spans));
        }

        if !slice.is_empty() {
            let span = Span::styled(slice.replace('\t', "    "), style);
            spans.push(span);
        }
    }

    if !spans.is_empty() {
        let spans = std::mem::take(&mut spans);
        lines.push(Spans::from(spans));
    }

    Text::from(lines)
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MarkdownSpan {
    pub text: String,
    pub style: Style,
    pub link: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RichLine(pub Vec<MarkdownSpan>);

pub struct Markdown {
    contents: String,

    config_loader: Arc<ArcSwap<syntax::Loader>>,
}

// TODO: pre-render and self reference via Pin
// better yet, just use Tendril + subtendril for references

impl Markdown {
    const TEXT_STYLE: &'static str = "ui.text";
    const BLOCK_STYLE: &'static str = "markup.raw.inline";
    const RULE_STYLE: &'static str = "punctuation.special";
    const UNNUMBERED_LIST_STYLE: &'static str = "markup.list.unnumbered";
    const NUMBERED_LIST_STYLE: &'static str = "markup.list.numbered";
    const HEADING_STYLES: [&'static str; 6] = [
        "markup.heading.1",
        "markup.heading.2",
        "markup.heading.3",
        "markup.heading.4",
        "markup.heading.5",
        "markup.heading.6",
    ];
    const INDENT: &'static str = "  ";
    const LINK_TEXT_STYLE: &'static str = "markup.link.text";
    const LINK_URL_STYLE: &'static str = "markup.link.url";

    pub fn new(contents: String, config_loader: Arc<ArcSwap<syntax::Loader>>) -> Self {
        Self {
            contents,
            config_loader,
        }
    }

    pub fn parse(&self, theme: Option<&Theme>) -> tui::text::Text<'_> {
        let lines = self
            .parse_rich(theme)
            .into_iter()
            .map(|RichLine(spans)| {
                Spans::from(
                    spans
                        .into_iter()
                        .map(|span| Span::styled(span.text, span.style))
                        .collect::<Vec<_>>(),
                )
            })
            .collect::<Vec<_>>();
        Text::from(lines)
    }

    pub fn parse_rich(&self, theme: Option<&Theme>) -> Vec<RichLine> {
        fn push_line(spans: &mut Vec<MarkdownSpan>, lines: &mut Vec<RichLine>) {
            let spans = std::mem::take(spans);
            if !spans.is_empty() {
                lines.push(RichLine(spans));
            }
        }

        let mut options = Options::empty();
        options.insert(Options::ENABLE_STRIKETHROUGH);
        let parser = Parser::new_ext(&self.contents, options);

        let mut tags = Vec::new();
        let mut spans = Vec::new();
        let mut lines = Vec::new();
        let mut list_stack = Vec::new();

        let get_indent = |level: usize| {
            if level < 1 {
                String::new()
            } else {
                Self::INDENT.repeat(level - 1)
            }
        };

        let get_theme = |key: &str| -> Style { theme.map(|t| t.get(key)).unwrap_or_default() };
        let text_style = get_theme(Self::TEXT_STYLE);
        let code_style = get_theme(Self::BLOCK_STYLE);
        let numbered_list_style = get_theme(Self::NUMBERED_LIST_STYLE);
        let unnumbered_list_style = get_theme(Self::UNNUMBERED_LIST_STYLE);
        let rule_style = get_theme(Self::RULE_STYLE);
        let link_text_style = {
            let style = get_theme(Self::LINK_TEXT_STYLE);
            if style == Style::default() {
                get_theme(Self::LINK_URL_STYLE)
            } else {
                style
            }
        };
        let heading_styles: Vec<Style> = Self::HEADING_STYLES
            .iter()
            .map(|key| get_theme(key))
            .collect();

        // Transform text in `<code>` blocks into `Event::Code`
        let mut in_code = false;
        let parser = parser.filter_map(|event| match event {
            Event::Html(tag)
                if tag.starts_with("<code") && matches!(tag.chars().nth(5), Some(' ' | '>')) =>
            {
                in_code = true;
                None
            }
            Event::Html(tag) if *tag == *"</code>" => {
                in_code = false;
                None
            }
            Event::Text(text) if in_code => Some(Event::Code(text)),
            _ => Some(event),
        });

        for event in parser {
            match event {
                Event::Start(Tag::List(list)) => {
                    if !list_stack.is_empty() {
                        push_line(&mut spans, &mut lines);
                    }

                    list_stack.push(list);
                }
                Event::End(TagEnd::List(_)) => {
                    list_stack.pop();

                    if list_stack.is_empty() {
                        lines.push(RichLine(Vec::new()));
                    }
                }
                Event::Start(Tag::Item) => {
                    if list_stack.is_empty() {
                        log::warn!("markdown parsing error, list item without list");
                    }

                    tags.push(Tag::Item);

                    let (bullet, bullet_style) = list_stack
                        .last()
                        .unwrap_or(&None)
                        .map_or((String::from("• "), unnumbered_list_style), |number| {
                            (format!("{}. ", number), numbered_list_style)
                        });

                    if let Some(v) = list_stack.last_mut().unwrap_or(&mut None).as_mut() {
                        *v += 1;
                    }

                    let prefix = get_indent(list_stack.len()) + bullet.as_str();
                    spans.push(MarkdownSpan {
                        text: prefix,
                        style: bullet_style,
                        link: None,
                    });
                }
                Event::Start(tag) => {
                    tags.push(tag);
                    if spans.is_empty() && !list_stack.is_empty() {
                        spans.push(MarkdownSpan {
                            text: get_indent(list_stack.len()),
                            style: text_style,
                            link: None,
                        });
                    }
                }
                Event::End(tag) => {
                    tags.pop();
                    match tag {
                        TagEnd::Heading(_)
                        | TagEnd::Paragraph
                        | TagEnd::CodeBlock
                        | TagEnd::Item => {
                            push_line(&mut spans, &mut lines);
                        }
                        _ => (),
                    }

                    match tag {
                        TagEnd::Heading(_) | TagEnd::Paragraph | TagEnd::CodeBlock => {
                            lines.push(RichLine(Vec::new()));
                        }
                        _ => (),
                    }
                }
                Event::Text(text) => {
                    if let Some(Tag::CodeBlock(kind)) = tags.last() {
                        let language = match kind {
                            CodeBlockKind::Fenced(language) => language,
                            CodeBlockKind::Indented => "",
                        };
                        let tui_text = highlighted_code_block(
                            &text,
                            language,
                            theme,
                            &self.config_loader.load(),
                            None,
                        );
                        lines.extend(tui_text.lines.into_iter().map(|line| {
                            RichLine(
                                line.0
                                    .into_iter()
                                    .map(|span| MarkdownSpan {
                                        text: span.content.into_owned(),
                                        style: span.style,
                                        link: None,
                                    })
                                    .collect(),
                            )
                        }));
                    } else {
                        let (style, detect_urls, active_link) = match tags.last() {
                            Some(Tag::Heading { level, .. }) => (
                                match level {
                                    HeadingLevel::H1 => heading_styles[0],
                                    HeadingLevel::H2 => heading_styles[1],
                                    HeadingLevel::H3 => heading_styles[2],
                                    HeadingLevel::H4 => heading_styles[3],
                                    HeadingLevel::H5 => heading_styles[4],
                                    HeadingLevel::H6 => heading_styles[5],
                                },
                                false,
                                None,
                            ),
                            Some(Tag::Emphasis) => (
                                text_style.add_modifier(Modifier::ITALIC),
                                false,
                                None,
                            ),
                            Some(Tag::Strong) => (text_style.add_modifier(Modifier::BOLD), false, None),
                            Some(Tag::Strikethrough) => (
                                text_style.add_modifier(Modifier::CROSSED_OUT),
                                false,
                                None,
                            ),
                            Some(Tag::Link { dest_url, .. }) => {
                                (link_text_style, false, Some(dest_url.to_string()))
                            }
                            _ => (text_style, true, None),
                        };
                        append_text_spans(
                            &mut spans,
                            text.into_string(),
                            style,
                            text_style,
                            link_text_style,
                            active_link.as_deref(),
                            detect_urls,
                        );
                    }
                }
                Event::Code(text) | Event::Html(text) => {
                    spans.push(MarkdownSpan {
                        text: text.into_string(),
                        style: code_style,
                        link: None,
                    });
                }
                Event::SoftBreak | Event::HardBreak => {
                    push_line(&mut spans, &mut lines);
                    if !list_stack.is_empty() {
                        spans.push(MarkdownSpan {
                            text: get_indent(list_stack.len()),
                            style: text_style,
                            link: None,
                        });
                    }
                }
                Event::Rule => {
                    lines.push(RichLine(vec![MarkdownSpan {
                        text: "───".into(),
                        style: rule_style,
                        link: None,
                    }]));
                    lines.push(RichLine(Vec::new()));
                }
                _ => {
                    log::warn!("unhandled markdown event {:?}", event);
                }
            }
        }

        if !spans.is_empty() {
            lines.push(RichLine(spans));
        }

        if matches!(lines.last(), Some(RichLine(spans)) if spans.is_empty()) {
            lines.pop();
        }

        lines
    }
}

fn append_text_spans(
    spans: &mut Vec<MarkdownSpan>,
    text: String,
    style: Style,
    text_style: Style,
    link_style: Style,
    active_link: Option<&str>,
    detect_urls: bool,
) {
    if let Some(dest) = active_link {
        spans.push(MarkdownSpan {
            text,
            style,
            link: Some(dest.to_string()),
        });
        return;
    }

    if detect_urls && style == text_style {
        spans.extend(split_https_urls(&text, text_style, link_style));
        return;
    }

    spans.push(MarkdownSpan {
        text,
        style,
        link: None,
    });
}

fn split_https_urls(text: &str, normal_style: Style, link_style: Style) -> Vec<MarkdownSpan> {
    let mut spans = Vec::new();
    let mut rest = text;
    while let Some(idx) = rest.find("https://") {
        if idx > 0 {
            spans.push(MarkdownSpan {
                text: rest[..idx].to_string(),
                style: normal_style,
                link: None,
            });
        }
        rest = &rest[idx..];
        let end = rest
            .char_indices()
            .skip(1)
            .find(|(_, ch)| {
                ch.is_whitespace() || matches!(ch, ')' | ']' | '>' | '"' | '\'' | ',')
            })
            .map(|(index, _)| index)
            .unwrap_or(rest.len());
        let url = &rest[..end];
        spans.push(MarkdownSpan {
            text: url.to_string(),
            style: link_style,
            link: Some(url.to_string()),
        });
        rest = &rest[end..];
    }

    if !rest.is_empty() {
        spans.push(MarkdownSpan {
            text: rest.to_string(),
            style: normal_style,
            link: None,
        });
    }

    spans
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    use arc_swap::ArcSwap;
    use helix_core::syntax;
    use helix_view::Theme;

    fn empty_loader() -> Arc<ArcSwap<syntax::Loader>> {
        let loader = syntax::Loader::new(syntax::config::Configuration {
            language: Vec::new(),
            language_server: HashMap::new(),
        })
        .expect("empty syntax loader");
        Arc::new(ArcSwap::from_pointee(loader))
    }

    #[test]
    fn parse_rich_marks_markdown_links() {
        let markdown = Markdown::new(
            "see [the docs](https://example.com/docs) here".into(),
            empty_loader(),
        );
        let lines = markdown.parse_rich(None);
        let linked: Vec<_> = lines
            .into_iter()
            .flat_map(|RichLine(spans)| spans)
            .filter(|span| span.link.is_some())
            .collect();
        assert_eq!(linked.len(), 1);
        assert_eq!(linked[0].text, "the docs");
        assert_eq!(linked[0].link.as_deref(), Some("https://example.com/docs"));
    }

    #[test]
    fn parse_rich_detects_bare_https_urls() {
        let markdown = Markdown::new(
            "visit https://example.com/docs for details".into(),
            empty_loader(),
        );
        let theme = Theme::default();
        let lines = markdown.parse_rich(Some(&theme));
        let linked: Vec<_> = lines
            .into_iter()
            .flat_map(|RichLine(spans)| spans)
            .filter(|span| span.link.is_some())
            .collect();
        assert_eq!(linked.len(), 1);
        assert_eq!(linked[0].text, "https://example.com/docs");
    }
}

impl Component for Markdown {
    fn render(&mut self, area: Rect, surface: &mut Surface, cx: &mut Context) {
        use tui::widgets::{Paragraph, Widget, Wrap};

        let text = self.parse(Some(&cx.editor.theme));

        let par = Paragraph::new(&text)
            .wrap(Wrap { trim: false })
            .scroll((cx.scroll.unwrap_or_default() as u16, 0));

        let margin = Margin::all(1);
        par.render(area.inner(margin), surface);
    }

    fn required_size(&mut self, viewport: (u16, u16)) -> Option<(u16, u16)> {
        let padding = 2;
        let contents = self.parse(None);

        // TODO: account for tab width
        let max_text_width = (viewport.0.saturating_sub(padding)).min(120);
        let (width, height) = crate::ui::text::required_size(&contents, max_text_width);

        Some((width + padding, height + padding))
    }
}
