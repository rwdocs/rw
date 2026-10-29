use std::panic::catch_unwind;

use pretty_assertions::assert_eq;
use rw_parser::Parser;

fn events(markdown: &str) -> Vec<String> {
    let mut parser = Parser::new(markdown, false, true);
    let mut events = Vec::new();
    while let Some(event) = parser.next() {
        events.push(format!("{event:?}"));
    }
    events
}

#[test]
fn reference_definition_continuations_do_not_panic() {
    for markdown in [
        "10. ![a](x.puml)\n > -    [r]:\n  x.puml\n         ",
        " \t:::note\n>1) [r]: x.puml 'T'\n    ",
        "- [r]: http://x\n      \n- b\n",
    ] {
        assert!(catch_unwind(|| events(markdown)).is_ok(), "{markdown:?}");
    }
}

#[test]
fn reference_definitions_preserve_following_list_items_and_links() {
    assert_eq!(
        events("- [r]: http://x\n      \n- b\n\n[r]\n"),
        [
            "Start(List(None))",
            "Start(Item)",
            "End(Item)",
            "Start(Item)",
            "Start(Paragraph { lone_image: false })",
            "Text(Borrowed(\"b\"))",
            "End(Paragraph)",
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

#[test]
fn reference_definition_whitespace_variations_do_not_panic() {
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
    for outer in outers {
        for marker in markers {
            for refdef in refdefs {
                for quote in quotes {
                    for blank in blanks {
                        for tail in tails {
                            let markdown = format!("{outer}{marker}{refdef}{quote}{blank}{tail}");
                            assert!(catch_unwind(|| events(&markdown)).is_ok(), "{markdown:?}");
                        }
                    }
                }
            }
        }
    }
}
