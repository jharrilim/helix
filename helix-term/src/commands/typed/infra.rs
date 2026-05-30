use std::borrow::Cow;
use std::collections::HashMap;
use std::fmt::Write;
use std::ops;

use crate::commands::prelude::*;
use crate::commands::Context;
use crate::ui::completers::Completer;
use helix_core::command_line::{self, Args, Flag, Signature, Token, TokenKind};
use helix_core::fuzzy::fuzzy_match;
use helix_view::expansion;

use super::{CommandCompleter, FocusRequirement, TypableCommand, TYPABLE_COMMAND_LIST};

pub static TYPABLE_COMMAND_MAP: Lazy<HashMap<&'static str, &'static TypableCommand>> =
    Lazy::new(|| {
        TYPABLE_COMMAND_LIST
            .iter()
            .flat_map(|cmd| {
                std::iter::once((cmd.name, cmd))
                    .chain(cmd.aliases.iter().map(move |&alias| (alias, cmd)))
            })
            .collect()
    });

fn execute_command_line(
    cx: &mut compositor::Context,
    input: &str,
    event: PromptEvent,
) -> anyhow::Result<()> {
    let (command, rest, _) = command_line::split(input);
    if command.is_empty() {
        return Ok(());
    }

    // If command is numeric, interpret as line number and go there.
    if command.parse::<usize>().is_ok() && rest.trim().is_empty() {
        let cmd = TYPABLE_COMMAND_MAP.get("goto").unwrap();
        return execute_command(cx, cmd, command, event);
    }

    match TYPABLE_COMMAND_MAP.get(command) {
        Some(cmd) => execute_command(cx, cmd, rest, event),
        None if event == PromptEvent::Validate => Err(anyhow!("no such command: '{command}'")),
        None => Ok(()),
    }
}

pub fn execute_command(
    cx: &mut compositor::Context,
    cmd: &TypableCommand,
    args: &str,
    event: PromptEvent,
) -> anyhow::Result<()> {
    let args = if event == PromptEvent::Validate {
        Args::parse(args, cmd.signature, true, |token| {
            expansion::expand(cx.editor, token).map_err(|err| err.into())
        })
        .map_err(|err| anyhow!("'{}': {err}", cmd.name))?
    } else {
        Args::parse(args, cmd.signature, false, |token| Ok(token.content))
            .expect("arg parsing cannot fail when validation is turned off")
    };

    if cmd.focus == FocusRequirement::Document && !cx.editor.is_document_view_focused() {
        match event {
            PromptEvent::Validate => {
                cx.editor.set_error("command requires a document view");
                return Ok(());
            }
            PromptEvent::Update | PromptEvent::Abort | PromptEvent::Advance => return Ok(()),
        }
    }

    (cmd.fun)(cx, args, event).map_err(|err| anyhow!("'{}': {err}", cmd.name))
}

#[allow(clippy::unnecessary_unwrap)]
pub fn command_mode(cx: &mut Context) {
    let mut prompt = Prompt::new(
        ":".into(),
        Some(':'),
        complete_command_line,
        move |cx: &mut compositor::Context, input: &str, event: PromptEvent| {
            if let Err(err) = execute_command_line(cx, input, event) {
                cx.editor.set_error(err.to_string());
            }
        },
    );
    prompt.doc_fn = Box::new(command_line_doc);

    // Calculate initial completion
    prompt.recalculate_completion(cx.editor);
    cx.push_layer(Box::new(prompt));
}

fn command_line_doc(input: &str) -> Option<Cow<'_, str>> {
    let (command, _, _) = command_line::split(input);
    let command = TYPABLE_COMMAND_MAP.get(command)?;

    if command.aliases.is_empty() && command.signature.flags.is_empty() {
        return Some(Cow::Borrowed(command.doc));
    }

    let mut doc = command.doc.to_string();

    if !command.aliases.is_empty() {
        write!(doc, "\nAliases: {}", command.aliases.join(", ")).unwrap();
    }

    if !command.signature.flags.is_empty() {
        const ARG_PLACEHOLDER: &str = " <arg>";

        fn flag_len(flag: &Flag) -> usize {
            let name_len = flag.name.len();
            let alias_len = if let Some(alias) = flag.alias {
                "/-".len() + alias.len_utf8()
            } else {
                0
            };
            let arg_len = if flag.completions.is_some() {
                ARG_PLACEHOLDER.len()
            } else {
                0
            };
            name_len + alias_len + arg_len
        }

        doc.push_str("\nFlags:");

        let max_flag_len = command.signature.flags.iter().map(flag_len).max().unwrap();

        for flag in command.signature.flags {
            let mut buf = [0u8; 4];
            let this_flag_len = flag_len(flag);
            write!(
                doc,
                "\n  --{flag_text}{spacer:spacing$}  {doc}",
                doc = flag.doc,
                // `fmt::Arguments` does not respect width controls so we must place the spacers
                // explicitly:
                spacer = "",
                spacing = max_flag_len - this_flag_len,
                flag_text = format_args!(
                    "{}{}{}{}",
                    flag.name,
                    // Ideally this would be written as a `format_args!` too but the borrow
                    // checker is not yet smart enough.
                    if flag.alias.is_some() { "/-" } else { "" },
                    if let Some(alias) = flag.alias {
                        alias.encode_utf8(&mut buf)
                    } else {
                        ""
                    },
                    if flag.completions.is_some() {
                        ARG_PLACEHOLDER
                    } else {
                        ""
                    }
                ),
            )
            .unwrap();
        }
    }

    Some(Cow::Owned(doc))
}

fn complete_command_line(editor: &Editor, input: &str) -> Vec<ui::prompt::Completion> {
    let (command, rest, complete_command) = command_line::split(input);

    if complete_command {
        fuzzy_match(
            input,
            TYPABLE_COMMAND_LIST.iter().map(|command| command.name),
            false,
        )
        .into_iter()
        .map(|(name, _)| (0.., name.into()))
        .collect()
    } else {
        TYPABLE_COMMAND_MAP
            .get(command)
            .map_or_else(Vec::new, |cmd| {
                let args_offset = command.len() + 1;
                complete_command_args(editor, cmd.signature, &cmd.completer, rest, args_offset)
            })
    }
}

pub fn complete_command_args(
    editor: &Editor,
    signature: Signature,
    completer: &CommandCompleter,
    input: &str,
    offset: usize,
) -> Vec<ui::prompt::Completion> {
    use command_line::{CompletionState, ExpansionKind, Tokenizer};

    // TODO: completion should depend on the location of the cursor instead of the end of the
    // string. This refactor is left for the future but the below completion code should respect
    // the cursor position if it becomes a parameter.
    let cursor = input.len();
    let prefix = &input[..cursor];
    let mut tokenizer = Tokenizer::new(prefix, false);
    let mut args = Args::new(signature, false);
    let mut final_token = None;
    let mut is_last_token = true;

    while let Some(token) = args
        .read_token(&mut tokenizer)
        .expect("arg parsing cannot fail when validation is turned off")
    {
        final_token = Some(token.clone());
        args.push(token.content)
            .expect("arg parsing cannot fail when validation is turned off");
        if tokenizer.pos() >= cursor {
            is_last_token = false;
        }
    }

    // Use a fake final token when the input is not terminated with a token. This simulates an
    // empty argument, causing completion on an empty value whenever you type space/tab. For
    // example if you say `":open README.md "` (with that trailing space) you should see the
    // files in the current dir - completing `""` rather than completions for `"README.md"` or
    // `"README.md "`.
    let token = if is_last_token {
        let token = Token::empty_at(prefix.len());
        args.push(token.content.clone()).unwrap();
        token
    } else {
        final_token.unwrap()
    };

    // Don't complete on closed tokens, for example after writing a closing double quote.
    if token.is_terminated {
        return Vec::new();
    }

    match token.kind {
        TokenKind::Unquoted | TokenKind::Quoted(_) => {
            match args.completion_state() {
                CompletionState::Positional => {
                    // If the completion state is positional there must be at least one positional
                    // in `args`.
                    let n = args
                        .len()
                        .checked_sub(1)
                        .expect("completion state to be positional");
                    let completer = completer.for_argument_number(n);

                    completer(editor, &token.content)
                        .into_iter()
                        .map(|(range, span)| quote_completion(&token, range, span, offset))
                        .collect()
                }
                CompletionState::Flag(_) => fuzzy_match(
                    token.content.trim_start_matches('-'),
                    signature.flags.iter().map(|flag| flag.name),
                    false,
                )
                .into_iter()
                .map(|(name, _)| ((offset + token.content_start).., format!("--{name}").into()))
                .collect(),
                CompletionState::FlagArgument(flag) => fuzzy_match(
                    &token.content,
                    flag.completions
                        .expect("flags in FlagArgument always have completions"),
                    false,
                )
                .into_iter()
                .map(|(value, _)| ((offset + token.content_start).., (*value).into()))
                .collect(),
            }
        }
        TokenKind::Expand | TokenKind::Expansion(ExpansionKind::Shell) => {
            // See the comment about the checked sub expect above.
            let arg_completer = matches!(args.completion_state(), CompletionState::Positional)
                .then(|| {
                    let n = args
                        .len()
                        .checked_sub(1)
                        .expect("completion state to be positional");
                    completer.for_argument_number(n)
                });
            complete_expand(editor, &token, arg_completer, offset + token.content_start)
        }
        TokenKind::Expansion(ExpansionKind::Variable) => {
            complete_variable_expansion(&token.content, offset + token.content_start)
        }
        TokenKind::Expansion(ExpansionKind::Unicode) => Vec::new(),
        TokenKind::Expansion(ExpansionKind::Register) => {
            complete_register_expansion(editor, &token.content, offset + token.content_start)
        }
        TokenKind::ExpansionKind => {
            complete_expansion_kind(&token.content, offset + token.content_start)
        }
    }
}

/// Replace the content and optionally update the range of a positional's completion to account
/// for quoting.
///
/// This is used to handle completions of file or directory names for example. When completing a
/// file with a space, tab or percent character in the name, the space should be escaped by
/// quoting the entire token. If the token being completed is already quoted, any quotes within
/// the completion text should be escaped by doubling them.
fn quote_completion<'a>(
    token: &Token,
    range: ops::RangeFrom<usize>,
    mut span: Span<'a>,
    offset: usize,
) -> (ops::RangeFrom<usize>, Span<'a>) {
    fn replace<'a>(text: Cow<'a, str>, from: char, to: &str) -> Cow<'a, str> {
        if text.contains(from) {
            Cow::Owned(text.replace(from, to))
        } else {
            text
        }
    }

    match token.kind {
        TokenKind::Unquoted if span.content.contains([' ', '\t', '%']) => {
            span.content = Cow::Owned(format!(
                "'{}{}'",
                // Escape any inner single quotes by doubling them.
                replace(token.content[..range.start].into(), '\'', "''"),
                replace(span.content, '\'', "''")
            ));
            // Ignore `range.start` here since we're replacing the entire token. We used
            // `range.start` above to emulate the replacement that using `range.start` would have
            // done.
            ((offset + token.content_start).., span)
        }
        TokenKind::Quoted(quote) => {
            span.content = replace(span.content, quote.char(), quote.escape());
            ((range.start + offset + token.content_start).., span)
        }
        TokenKind::Expand => {
            // NOTE: `token.content_start` is already accounted for in `offset` for `Expand`
            // tokens.
            span.content = replace(span.content, '"', "\"\"");
            ((range.start + offset).., span)
        }
        _ => ((range.start + offset + token.content_start).., span),
    }
}

fn complete_expand(
    editor: &Editor,
    token: &Token,
    completer: Option<&Completer>,
    offset: usize,
) -> Vec<ui::prompt::Completion> {
    use command_line::{ExpansionKind, Tokenizer};

    let mut start = 0;

    // If the expand token contains expansions, complete those.
    while let Some(idx) = token.content[start..].find('%') {
        let idx = start + idx;
        if token.content.as_bytes().get(idx + '%'.len_utf8()).copied() == Some(b'%') {
            // Two percents together are skipped.
            start = idx + ('%'.len_utf8() * 2);
        } else {
            let mut tokenizer = Tokenizer::new(&token.content[idx..], false);
            let token = tokenizer
                .parse_percent_token()
                .map(|token| token.expect("arg parser cannot fail when validation is disabled"));
            start = idx + tokenizer.pos();

            // Like closing quote characters in `complete_command_args` above, don't provide
            // completions if the token is already terminated. This also skips expansions
            // which have already been fully written, for example
            // `"%{cursor_line}:%{cursor_col` should complete `cursor_column` instead of
            // `cursor_line`.
            let Some(token) = token.filter(|t| !t.is_terminated) else {
                continue;
            };

            let local_offset = offset + idx + token.content_start;
            match token.kind {
                TokenKind::Expansion(ExpansionKind::Variable) => {
                    return complete_variable_expansion(&token.content, local_offset);
                }
                TokenKind::Expansion(ExpansionKind::Shell) => {
                    return complete_expand(editor, &token, None, local_offset);
                }
                TokenKind::ExpansionKind => {
                    return complete_expansion_kind(&token.content, local_offset);
                }
                _ => continue,
            }
        }
    }

    match completer {
        // If no expansions were found and an argument is being completed,
        Some(completer) if start == 0 => completer(editor, &token.content)
            .into_iter()
            .map(|(range, span)| quote_completion(token, range, span, offset))
            .collect(),
        _ => Vec::new(),
    }
}

fn complete_variable_expansion(content: &str, offset: usize) -> Vec<ui::prompt::Completion> {
    use expansion::Variable;

    fuzzy_match(
        content,
        Variable::VARIANTS.iter().map(Variable::as_str),
        false,
    )
    .into_iter()
    .map(|(name, _)| (offset.., (*name).into()))
    .collect()
}

fn complete_register_expansion(
    editor: &Editor,
    content: &str,
    offset: usize,
) -> Vec<ui::prompt::Completion> {
    let register_names: Vec<String> = editor
        .registers
        .iter_preview()
        .map(|(ch, _)| ch.to_string())
        .collect();
    fuzzy_match(content, register_names, false)
        .into_iter()
        .map(|(name, _)| (offset.., name.to_string().into()))
        .collect()
}

fn complete_expansion_kind(content: &str, offset: usize) -> Vec<ui::prompt::Completion> {
    use command_line::ExpansionKind;

    fuzzy_match(
        content,
        // Skip `ExpansionKind::Variable` since its kind string is empty.
        ExpansionKind::VARIANTS
            .iter()
            .skip(1)
            .map(ExpansionKind::as_str),
        false,
    )
    .into_iter()
    .map(|(name, _)| (offset.., (*name).into()))
    .collect()
}
