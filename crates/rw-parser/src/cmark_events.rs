//! cmark's offset iterator, made to survive a panic in pulldown-cmark 0.13.4.

use std::ops::Range;
use std::panic::{AssertUnwindSafe, catch_unwind};

use pulldown_cmark::{Event, OffsetIter, Options, Parser};

#[cfg(not(panic = "unwind"))]
compile_error!("rw-parser catches a pulldown-cmark panic, which needs `panic = \"unwind\"`");

/// [`OffsetIter`] that steps over cmark's empty tight paragraphs.
///
/// A link reference definition alone in a tight list item, followed by a
/// whitespace-only line cmark reads as a lazy continuation (one indented four
/// columns past the item's content, say), leaves cmark an empty tight
/// paragraph. `OffsetIter::next` unwraps its missing first child and panics.
/// By then it has only moved its cursor into the paragraph, so the next call
/// pops back out and goes on as if the paragraph were not there.
///
/// One retry, uncaught: such a paragraph is never followed by another, since
/// two sibling paragraphs need a blank line between them, which makes the list
/// loose. A second panic is some other bug.
///
/// The catch can't tell this panic from another; cmark 0.13.4 has no other
/// known `next()` panic (5M-input fuzz). A cmark upgrade must re-run this
/// module's canary and sweep tests.
pub(crate) struct CmarkEvents<'a> {
    inner: OffsetIter<'a>,
    /// Set only for a source with a `]:`, which every reference definition
    /// holds: the catch costs a few percent of a render.
    guarded: bool,
}

impl<'a> CmarkEvents<'a> {
    pub(crate) fn new(markdown: &'a str, options: Options) -> Self {
        Self {
            inner: Parser::new_ext(markdown, options).into_offset_iter(),
            guarded: markdown.contains("]:"),
        }
    }

    #[inline(never)]
    fn next_guarded(&mut self) -> Option<(Event<'a>, Range<usize>)> {
        catch_unwind(AssertUnwindSafe(|| self.inner.next())).unwrap_or_else(|_| self.inner.next())
    }
}

impl<'a> Iterator for CmarkEvents<'a> {
    type Item = (Event<'a>, Range<usize>);

    fn next(&mut self) -> Option<Self::Item> {
        if self.guarded {
            return self.next_guarded();
        }
        self.inner.next()
    }
}

#[cfg(test)]
mod tests {
    use std::panic::{AssertUnwindSafe, catch_unwind};

    use pretty_assertions::assert_eq;
    use pulldown_cmark::{Options, Parser as CmarkParser};

    use crate::parser::tests::debug_stream;

    /// The issue's two inputs, and the shortest known trigger.
    const TRIGGERS: [&str; 3] = [
        "10. ![a](x.puml)\n > -    [r]:\n  x.puml\n         ",
        " \t:::note\n>1) [r]: x.puml 'T'\n    ",
        "- [r]: http://x\n      \n- b\n",
    ];

    fn cmark_panics(markdown: &str) -> bool {
        catch_unwind(|| {
            CmarkParser::new_ext(markdown, Options::empty())
                .into_offset_iter()
                .count()
        })
        .is_err()
    }

    /// Once this fails, cmark is fixed and `CmarkEvents` can go.
    #[test]
    fn cmark_still_panics_on_the_triggers() {
        for markdown in TRIGGERS {
            assert!(cmark_panics(markdown), "{markdown:?}");
        }
    }

    #[test]
    fn the_empty_paragraph_is_skipped_and_the_page_goes_on() {
        assert_eq!(
            debug_stream("- [r]: http://x\n      \n- b\n\n[r]\n"),
            [
                "Start(List(None))",
                "Start(Item)",
                "End(Item)",
                "Start(Item)",
                "Text(Borrowed(\"b\"))",
                "End(Item)",
                "End(List(false))",
                "Start(Paragraph { lone_image: false })",
                "Start(Link { kind: Other, dest_url: Borrowed(\"http://x\") })",
                "Text(Borrowed(\"r\"))",
                "End(Link)",
                "End(Paragraph)",
            ]
        );
    }

    /// Variations on the trigger: containers, list markers, refdef forms and
    /// line endings, and the whitespace-only line and what follows it.
    ///
    /// Kept small: under `RUST_BACKTRACE=1` every caught panic captures a
    /// backtrace.
    #[test]
    fn no_variation_of_the_trigger_panics() {
        let outers = ["", "> ", " > ", "- ", "1. a\n", "  "];
        let markers = ["- ", "* ", "1. ", "10) ", "-    ", "- - "];
        let refdefs = [
            "[r]: http://x\n",
            "[r]:\n  x.puml\r\n",
            "[r]: x.puml 'T'\r",
            "[r]: <x>\r\n  \"t\"\n",
        ];
        let quotes = ["", ">"];
        let blanks = [
            "",
            " ",
            "    ",
            "      ",
            "        ",
            "          ",
            "\t",
            " \t",
            "\t\t",
            "\x0c",
            "  \x0b",
        ];
        let tails = [
            "",
            "\n",
            "\n- b\n",
            "\n\n- b\n",
            "\n  c\n",
            "\n[r]\n",
            "\r\n- b",
            "\r- b\r",
        ];
        let mut triggers = 0;
        for outer in outers {
            for marker in markers {
                for refdef in refdefs {
                    for quote in quotes {
                        for blank in blanks {
                            for tail in tails {
                                let markdown =
                                    format!("{outer}{marker}{refdef}{quote}{blank}{tail}");
                                triggers += usize::from(cmark_panics(&markdown));
                                let parsed =
                                    catch_unwind(AssertUnwindSafe(|| debug_stream(&markdown)));
                                assert!(parsed.is_ok(), "{markdown:?}");
                            }
                        }
                    }
                }
            }
        }
        assert!(triggers > 5_000, "only {triggers} variations reach the bug");
    }
}
