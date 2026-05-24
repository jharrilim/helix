use arc_swap::ArcSwap;
use helix_core::syntax;
use helix_core::{encoding, Rope, Selection, Transaction};
use std::path::PathBuf;
use std::sync::Arc;

use crate::editor::Config;
use crate::{Document, ViewId};

use super::{from_reader, to_writer};

    #[test]
    fn changeset_to_changes_ignore_line_endings() {
        use helix_lsp::{lsp, Client, OffsetEncoding};
        let text = Rope::from("hello\r\nworld");
        let mut doc = Document::from(
            text,
            None,
            Arc::new(ArcSwap::new(Arc::new(Config::default()))),
            Arc::new(ArcSwap::from_pointee(syntax::Loader::default())),
        );
        let view = ViewId::default();
        doc.set_selection(view, Selection::single(0, 0));

        let transaction =
            Transaction::change(doc.text(), vec![(5, 7, Some("\n".into()))].into_iter());
        let old_doc = doc.text().clone();
        doc.apply(&transaction, view);
        let changes = Client::changeset_to_changes(
            &old_doc,
            doc.text(),
            transaction.changes(),
            OffsetEncoding::Utf8,
        );

        assert_eq!(doc.text(), "hello\nworld");

        assert_eq!(
            changes,
            &[lsp::TextDocumentContentChangeEvent {
                range: Some(lsp::Range::new(
                    lsp::Position::new(0, 5),
                    lsp::Position::new(1, 0)
                )),
                text: "\n".into(),
                range_length: None,
            }]
        );
    }

    #[test]
    fn changeset_to_changes() {
        use helix_lsp::{lsp, Client, OffsetEncoding};
        let text = Rope::from("hello");
        let mut doc = Document::from(
            text,
            None,
            Arc::new(ArcSwap::new(Arc::new(Config::default()))),
            Arc::new(ArcSwap::from_pointee(syntax::Loader::default())),
        );
        let view = ViewId::default();
        doc.set_selection(view, Selection::single(5, 5));

        // insert

        let transaction = Transaction::insert(doc.text(), doc.selection(view), " world".into());
        let old_doc = doc.text().clone();
        doc.apply(&transaction, view);
        let changes = Client::changeset_to_changes(
            &old_doc,
            doc.text(),
            transaction.changes(),
            OffsetEncoding::Utf8,
        );

        assert_eq!(
            changes,
            &[lsp::TextDocumentContentChangeEvent {
                range: Some(lsp::Range::new(
                    lsp::Position::new(0, 5),
                    lsp::Position::new(0, 5)
                )),
                text: " world".into(),
                range_length: None,
            }]
        );

        // delete

        let transaction = transaction.invert(&old_doc);
        let old_doc = doc.text().clone();
        doc.apply(&transaction, view);
        let changes = Client::changeset_to_changes(
            &old_doc,
            doc.text(),
            transaction.changes(),
            OffsetEncoding::Utf8,
        );

        // line: 0-based.
        // col: 0-based, gaps between chars.
        // 0 1 2 3 4 5 6 7 8 9 0 1
        // |h|e|l|l|o| |w|o|r|l|d|
        //           -------------
        // (0, 5)-(0, 11)
        assert_eq!(
            changes,
            &[lsp::TextDocumentContentChangeEvent {
                range: Some(lsp::Range::new(
                    lsp::Position::new(0, 5),
                    lsp::Position::new(0, 11)
                )),
                text: "".into(),
                range_length: None,
            }]
        );

        // replace

        // also tests that changes are layered, positions depend on previous changes.

        doc.set_selection(view, Selection::single(0, 5));
        let transaction = Transaction::change(
            doc.text(),
            vec![(0, 2, Some("aei".into())), (3, 5, Some("ou".into()))].into_iter(),
        );
        // aeilou
        let old_doc = doc.text().clone();
        doc.apply(&transaction, view);
        let changes = Client::changeset_to_changes(
            &old_doc,
            doc.text(),
            transaction.changes(),
            OffsetEncoding::Utf8,
        );

        assert_eq!(
            changes,
            &[
                // 0 1 2 3 4 5
                // |h|e|l|l|o|
                // ----
                //
                // aeillo
                lsp::TextDocumentContentChangeEvent {
                    range: Some(lsp::Range::new(
                        lsp::Position::new(0, 0),
                        lsp::Position::new(0, 2)
                    )),
                    text: "aei".into(),
                    range_length: None,
                },
                // 0 1 2 3 4 5 6
                // |a|e|i|l|l|o|
                //         -----
                //
                // aeilou
                lsp::TextDocumentContentChangeEvent {
                    range: Some(lsp::Range::new(
                        lsp::Position::new(0, 4),
                        lsp::Position::new(0, 6)
                    )),
                    text: "ou".into(),
                    range_length: None,
                }
            ]
        );
    }

    #[test]
    fn test_line_ending() {
        assert_eq!(
            Document::default(
                Arc::new(ArcSwap::new(Arc::new(Config::default()))),
                Arc::new(ArcSwap::from_pointee(syntax::Loader::default()))
            )
            .text()
            .to_string(),
            helix_core::NATIVE_LINE_ENDING.as_str()
        );
    }

    macro_rules! decode {
        ($name:ident, $label:expr, $label_override:expr) => {
            #[test]
            fn $name() {
                let encoding = encoding::Encoding::for_label($label_override.as_bytes()).unwrap();
                let base_path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/encoding");
                let path = base_path.join(format!("{}_in.txt", $label));
                let ref_path = base_path.join(format!("{}_in_ref.txt", $label));
                assert!(path.exists());
                assert!(ref_path.exists());

                let mut file = std::fs::File::open(path).unwrap();
                let text = from_reader(&mut file, Some(encoding.into()))
                    .unwrap()
                    .0
                    .to_string();
                let expectation = std::fs::read_to_string(ref_path).unwrap();
                assert_eq!(text[..], expectation[..]);
            }
        };
        ($name:ident, $label:expr) => {
            decode!($name, $label, $label);
        };
    }

    macro_rules! encode {
        ($name:ident, $label:expr, $label_override:expr) => {
            #[test]
            fn $name() {
                let encoding = encoding::Encoding::for_label($label_override.as_bytes()).unwrap();
                let base_path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/encoding");
                let path = base_path.join(format!("{}_out.txt", $label));
                let ref_path = base_path.join(format!("{}_out_ref.txt", $label));
                assert!(path.exists());
                assert!(ref_path.exists());

                let text = Rope::from_str(&std::fs::read_to_string(path).unwrap());
                let mut buf: Vec<u8> = Vec::new();
                helix_lsp::block_on(to_writer(&mut buf, (encoding, false), &text)).unwrap();

                let expectation = std::fs::read(ref_path).unwrap();
                assert_eq!(buf, expectation);
            }
        };
        ($name:ident, $label:expr) => {
            encode!($name, $label, $label);
        };
    }

    decode!(big5_decode, "big5");
    encode!(big5_encode, "big5");
    decode!(euc_kr_decode, "euc_kr", "EUC-KR");
    encode!(euc_kr_encode, "euc_kr", "EUC-KR");
    decode!(gb18030_decode, "gb18030");
    encode!(gb18030_encode, "gb18030");
    decode!(iso_2022_jp_decode, "iso_2022_jp", "ISO-2022-JP");
    encode!(iso_2022_jp_encode, "iso_2022_jp", "ISO-2022-JP");
    decode!(jis0208_decode, "jis0208", "EUC-JP");
    encode!(jis0208_encode, "jis0208", "EUC-JP");
    decode!(jis0212_decode, "jis0212", "EUC-JP");
    decode!(shift_jis_decode, "shift_jis");
    encode!(shift_jis_encode, "shift_jis");
