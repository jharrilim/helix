use helix_core::chars::char_is_word;
use helix_core::diagnostic::DiagnosticProvider;
use helix_core::{Diagnostic, Rope};
use helix_lsp::util::lsp_pos_to_pos;
use helix_lsp::{lsp, LanguageServerId};
use helix_core::syntax::config::LanguageConfiguration;

use super::Document;

impl Document {
    pub fn lsp_diagnostic_to_diagnostic(
        text: &Rope,
        language_config: Option<&LanguageConfiguration>,
        diagnostic: &helix_lsp::lsp::Diagnostic,
        provider: DiagnosticProvider,
        offset_encoding: helix_lsp::OffsetEncoding,
    ) -> Option<Diagnostic> {
        use helix_core::diagnostic::{Range, Severity::*};

        // TODO: convert inside server
        let start =
            if let Some(start) = lsp_pos_to_pos(text, diagnostic.range.start, offset_encoding) {
                start
            } else {
                log::warn!("lsp position out of bounds - {:?}", diagnostic);
                return None;
            };

        let end = if let Some(end) = lsp_pos_to_pos(text, diagnostic.range.end, offset_encoding) {
            end
        } else {
            log::warn!("lsp position out of bounds - {:?}", diagnostic);
            return None;
        };

        let severity = diagnostic.severity.and_then(|severity| match severity {
            lsp::DiagnosticSeverity::ERROR => Some(Error),
            lsp::DiagnosticSeverity::WARNING => Some(Warning),
            lsp::DiagnosticSeverity::INFORMATION => Some(Info),
            lsp::DiagnosticSeverity::HINT => Some(Hint),
            severity => {
                log::error!("unrecognized diagnostic severity: {:?}", severity);
                None
            }
        });

        if let Some(lang_conf) = language_config {
            if let Some(severity) = severity {
                if severity < lang_conf.diagnostic_severity {
                    return None;
                }
            }
        };
        use helix_core::diagnostic::{DiagnosticTag, NumberOrString};

        let code = match diagnostic.code.clone() {
            Some(x) => match x {
                lsp::NumberOrString::Number(x) => Some(NumberOrString::Number(x)),
                lsp::NumberOrString::String(x) => Some(NumberOrString::String(x)),
            },
            None => None,
        };

        let tags = if let Some(tags) = &diagnostic.tags {
            let new_tags = tags
                .iter()
                .filter_map(|tag| match *tag {
                    lsp::DiagnosticTag::DEPRECATED => Some(DiagnosticTag::Deprecated),
                    lsp::DiagnosticTag::UNNECESSARY => Some(DiagnosticTag::Unnecessary),
                    _ => None,
                })
                .collect();

            new_tags
        } else {
            Vec::new()
        };

        let ends_at_word =
            start != end && end != 0 && text.get_char(end - 1).is_some_and(char_is_word);
        let starts_at_word = start != end && text.get_char(start).is_some_and(char_is_word);

        Some(Diagnostic {
            range: Range { start, end },
            ends_at_word,
            starts_at_word,
            zero_width: start == end,
            line: diagnostic.range.start.line as usize,
            message: diagnostic.message.clone(),
            severity,
            code,
            tags,
            source: diagnostic.source.clone(),
            data: diagnostic.data.clone(),
            provider,
        })
    }

    #[inline]
    pub fn diagnostics(&self) -> &[Diagnostic] {
        &self.diagnostics
    }

    pub fn replace_diagnostics(
        &mut self,
        diagnostics: impl IntoIterator<Item = Diagnostic>,
        unchanged_sources: &[String],
        provider: Option<&DiagnosticProvider>,
    ) {
        if unchanged_sources.is_empty() {
            if let Some(provider) = provider {
                self.diagnostics
                    .retain(|diagnostic| &diagnostic.provider != provider);
            } else {
                self.diagnostics.clear();
            }
        } else {
            self.diagnostics.retain(|d| {
                if provider.is_some_and(|provider| provider != &d.provider) {
                    return true;
                }

                if let Some(source) = &d.source {
                    unchanged_sources.contains(source)
                } else {
                    false
                }
            });
        }
        self.diagnostics.extend(diagnostics);
        self.diagnostics.sort_by_key(|diagnostic| {
            (
                diagnostic.range,
                diagnostic.severity,
                diagnostic.provider.clone(),
            )
        });
    }

    /// clears diagnostics for a given language server id if set, otherwise all diagnostics are cleared
    pub fn clear_diagnostics_for_language_server(&mut self, id: LanguageServerId) {
        self.diagnostics
            .retain(|d| d.provider.language_server_id() != Some(id));
    }
}
