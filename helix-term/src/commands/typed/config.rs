use std::ops::Deref;
use super::super::prelude::*;
use helix_core::command_line::{self, Args};
use helix_view::{editor::ConfigEvent, theme};
use serde_json::Value;
pub(crate) fn theme(cx: &mut compositor::Context, args: Args, event: PromptEvent) -> anyhow::Result<()> {
    let true_color = cx.editor.config.load().true_color || crate::true_color();
    match event {
        PromptEvent::Abort => {
            cx.editor.unset_theme_preview()?;
        }
        PromptEvent::Update => {
            if args.is_empty() {
                // Ensures that a preview theme gets cleaned up if the user backspaces until the prompt is empty.
                cx.editor.unset_theme_preview()?;
            } else if let Some(theme_name) = args.first() {
                if let Ok(theme) = cx.editor.theme_loader.load(theme_name) {
                    if !(true_color || theme.is_16_color()) {
                        bail!("Unsupported theme: theme requires true color support");
                    }
                    cx.editor.set_theme_preview(theme)?;
                };
            };
        }
        PromptEvent::Validate => {
            if let Some(theme_name) = args.first() {
                let theme = cx
                    .editor
                    .theme_loader
                    .load(theme_name)
                    .map_err(|err| anyhow::anyhow!("Could not load theme: {}", err))?;
                if !(true_color || theme.is_16_color()) {
                    bail!("Unsupported theme: theme requires true color support");
                }
                cx.editor.set_theme(theme)?;

                let config_path = helix_loader::config_file();
                match theme::Config::save_selection(&config_path, theme_name, None) {
                    Ok(theme_config) => {
                        cx.editor
                            .config_events
                            .0
                            .send(ConfigEvent::SetTheme(theme_config))?;
                    }
                    Err(err) => {
                        cx.editor.set_error(format!(
                            "Theme applied but failed to save to config: {err}"
                        ));
                    }
                }
            } else {
                let name = cx.editor.theme.name().to_string();

                cx.editor.set_status(name);
            }
        }
    };

    Ok(())
}
pub(crate) fn get_option(cx: &mut compositor::Context, args: Args, event: PromptEvent) -> anyhow::Result<()> {
    if event != PromptEvent::Validate {
        return Ok(());
    }

    let key = &args[0].to_lowercase();
    let key_error = || anyhow::anyhow!("Unknown key `{}`", key);

    let config = serde_json::json!(cx.editor.config().deref());
    let pointer = format!("/{}", key.replace('.', "/"));
    let value = config.pointer(&pointer).ok_or_else(key_error)?;

    cx.editor.set_status(value.to_string());
    Ok(())
}

/// Change config at runtime. Access nested values by dot syntax, for
/// example to disable smart case search, use `:set search.smart-case false`.
pub(crate) fn set_option(cx: &mut compositor::Context, args: Args, event: PromptEvent) -> anyhow::Result<()> {
    if event != PromptEvent::Validate {
        return Ok(());
    }

    let (key, arg) = (&args[0].to_lowercase(), args[1].trim());

    let key_error = || anyhow::anyhow!("Unknown key `{}`", key);
    let field_error = |_| anyhow::anyhow!("Could not parse field `{}`", arg);

    let mut config = serde_json::json!(&cx.editor.config().deref());
    let pointer = format!("/{}", key.replace('.', "/"));
    let value = config.pointer_mut(&pointer).ok_or_else(key_error)?;

    *value = if value.is_string() {
        // JSON strings require quotes, so we can't .parse() directly
        Value::String(arg.to_string())
    } else {
        arg.parse().map_err(field_error)?
    };
    let config = serde_json::from_value(config).map_err(field_error)?;

    cx.editor
        .config_events
        .0
        .send(ConfigEvent::Update(config))?;
    Ok(())
}

/// Toggle boolean config option at runtime. Access nested values by dot
/// syntax, for example to toggle smart case search, use `:toggle search.smart-
/// case`.
pub(crate) fn toggle_option(
    cx: &mut compositor::Context,
    args: Args,
    event: PromptEvent,
) -> anyhow::Result<()> {
    if event != PromptEvent::Validate {
        return Ok(());
    }

    let key = &args[0].to_lowercase();

    let key_error = || anyhow::anyhow!("Unknown key `{}`", key);

    let mut config = serde_json::json!(&cx.editor.config().deref());
    let pointer = format!("/{}", key.replace('.', "/"));
    let value = config.pointer_mut(&pointer).ok_or_else(key_error)?;

    *value = match value {
        Value::Bool(ref value) => {
            ensure!(
                args.len() == 1,
                "Bad arguments. For boolean configurations use: `:toggle {key}`"
            );
            Value::Bool(!value)
        }
        Value::String(ref value) => {
            ensure!(
                args.len() == 2,
                "Bad arguments. For string configurations use: `:toggle {key} val1 val2 ...`",
            );
            // For string values, parse the input according to normal command line rules.
            let values: Vec<_> = command_line::Tokenizer::new(&args[1], true)
                .map(|res| res.map(|token| token.content))
                .collect::<Result<_, _>>()
                .map_err(|err| anyhow!("failed to parse values: {err}"))?;

            Value::String(
                values
                    .iter()
                    .skip_while(|e| *e != value)
                    .nth(1)
                    .map(AsRef::as_ref)
                    .unwrap_or_else(|| &values[0])
                    .to_string(),
            )
        }
        Value::Null => bail!("Configuration {key} cannot be toggled"),
        Value::Number(_) | Value::Array(_) | Value::Object(_) => {
            ensure!(
                args.len() == 2,
                "Bad arguments. For {kind} configurations use: `:toggle {key} val1 val2 ...`",
                kind = match value {
                    Value::Number(_) => "number",
                    Value::Array(_) => "array",
                    Value::Object(_) => "object",
                    _ => unreachable!(),
                }
            );
            // For numbers, arrays and objects, parse each argument with
            // `serde_json::StreamDeserializer`.
            let values: Vec<Value> = serde_json::Deserializer::from_str(&args[1])
                .into_iter()
                .collect::<Result<_, _>>()
                .map_err(|err| anyhow!("failed to parse value: {err}"))?;

            if let Some(wrongly_typed_value) = values
                .iter()
                .find(|v| std::mem::discriminant(*v) != std::mem::discriminant(&*value))
            {
                bail!("value '{wrongly_typed_value}' has a different type than '{value}'");
            }

            values
                .iter()
                .skip_while(|e| *e != value)
                .nth(1)
                .unwrap_or(&values[0])
                .clone()
        }
    };

    let status = format!("'{key}' is now set to {value}");
    let config = serde_json::from_value(config)
        .map_err(|err| anyhow::anyhow!("Failed to parse config: {err}"))?;

    cx.editor
        .config_events
        .0
        .send(ConfigEvent::Update(config))?;
    cx.editor.set_status(status);
    Ok(())
}
pub(crate) fn open_config(
    cx: &mut compositor::Context,
    _args: Args,
    event: PromptEvent,
) -> anyhow::Result<()> {
    if event != PromptEvent::Validate {
        return Ok(());
    }

    cx.editor
        .open(&helix_loader::config_file(), Action::Replace)?;
    Ok(())
}

pub(crate) fn open_workspace_config(
    cx: &mut compositor::Context,
    _args: Args,
    event: PromptEvent,
) -> anyhow::Result<()> {
    if event != PromptEvent::Validate {
        return Ok(());
    }

    cx.editor
        .open(&helix_loader::workspace_config_file(), Action::Replace)?;
    Ok(())
}

pub(crate) fn open_log(cx: &mut compositor::Context, _args: Args, event: PromptEvent) -> anyhow::Result<()> {
    if event != PromptEvent::Validate {
        return Ok(());
    }

    cx.editor.open(&helix_loader::log_file(), Action::Replace)?;
    Ok(())
}

pub(crate) fn refresh_config(
    cx: &mut compositor::Context,
    _args: Args,
    event: PromptEvent,
) -> anyhow::Result<()> {
    if event != PromptEvent::Validate {
        return Ok(());
    }

    cx.editor.config_events.0.send(ConfigEvent::Refresh)?;
    Ok(())
}

