use php_syntax::{Parse, SyntaxElement, parse_source_file};
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::sync::mpsc;
use std::time::Duration;

const DEFAULT_CASES: usize = 128;
const DEFAULT_TIMEOUT_MS: u64 = 2_000;

#[test]
fn parser_property_smoke_is_lossless_and_bounded() {
    run_property_cases(DEFAULT_CASES, 96, Duration::from_millis(DEFAULT_TIMEOUT_MS));
}

#[test]
#[ignore]
fn parser_long_fuzz_smoke_is_lossless_and_bounded() {
    let cases = std::env::var("PARSER_FUZZ_CASES")
        .ok()
        .and_then(|value| value.parse().ok())
        .unwrap_or(1024);
    run_property_cases(cases, 256, Duration::from_millis(5_000));
}

fn run_property_cases(generated_cases: usize, max_len: usize, timeout: Duration) {
    for source in fixed_cases() {
        assert_parse_properties(source, timeout);
    }

    for bytes in generated_byte_cases(generated_cases, max_len) {
        let source = String::from_utf8_lossy(&bytes).into_owned();
        assert_parse_properties(&source, timeout);
    }
}

fn fixed_cases() -> &'static [&'static str] {
    &[
        "",
        "plain inline html",
        "<?php",
        "<?php echo 1;",
        "<?php echo ;",
        "<?php function f(#[A(] $x): ?| {}",
        "<?php class C { public function f(, $x = ];",
        "<?php match ($x) { 1 => , default => 2,",
        "<?php \"unterminated",
        "<?php <<<TXT\nhello {$name}\nTXT;\n",
        "<?php $object->match(); C::readonly;",
        "\0\0<?php echo @@@ ;",
    ]
}

fn assert_parse_properties(source: &str, timeout: Duration) {
    let summary = parse_with_timeout(source, timeout);

    assert_eq!(
        summary.reconstructed_text, source,
        "parser must preserve source text for {source:?}"
    );
    assert!(
        summary.root_start <= summary.root_end && summary.root_end <= source.len(),
        "root range must stay within source for {source:?}: {:?}",
        (summary.root_start, summary.root_end)
    );

    for span in summary.diagnostic_spans {
        assert!(
            span.0 <= span.1,
            "diagnostic span start must be <= end for {source:?}: {span:?}"
        );
        assert!(
            span.1 <= source.len(),
            "diagnostic span must be within source for {source:?}: {span:?}"
        );
    }

    for span in summary.element_spans {
        assert!(
            span.0 <= span.1,
            "CST span start must be <= end for {source:?}: {span:?}"
        );
        assert!(
            span.1 <= source.len(),
            "CST span must be within source for {source:?}: {span:?}"
        );
    }
}

fn parse_with_timeout(source: &str, timeout: Duration) -> ParseSummary {
    let source = source.to_owned();
    let (sender, receiver) = mpsc::channel();
    std::thread::spawn(move || {
        let result = catch_unwind(AssertUnwindSafe(|| summarize_parse(&source)));
        let _sent = sender.send(result);
    });

    match receiver.recv_timeout(timeout) {
        Ok(Ok(summary)) => summary,
        Ok(Err(_panic)) => panic!("parser panicked"),
        Err(_timeout) => panic!("parser did not terminate within {timeout:?}"),
    }
}

fn summarize_parse(source: &str) -> ParseSummary {
    let parse = parse_source_file(source);
    ParseSummary::from_parse(parse)
}

#[derive(Debug)]
struct ParseSummary {
    reconstructed_text: String,
    root_start: usize,
    root_end: usize,
    diagnostic_spans: Vec<(usize, usize)>,
    element_spans: Vec<(usize, usize)>,
}

impl ParseSummary {
    fn from_parse(parse: Parse) -> Self {
        let mut element_spans = Vec::new();
        collect_element_spans(parse.root().children(), &mut element_spans);
        Self {
            reconstructed_text: parse.reconstructed_text().to_owned(),
            root_start: parse.root().range().start().to_usize(),
            root_end: parse.root().range().end().to_usize(),
            diagnostic_spans: parse
                .diagnostics()
                .iter()
                .map(|diagnostic| {
                    (
                        diagnostic.span.start().to_usize(),
                        diagnostic.span.end().to_usize(),
                    )
                })
                .collect(),
            element_spans,
        }
    }
}

fn collect_element_spans(elements: &[SyntaxElement], out: &mut Vec<(usize, usize)>) {
    for element in elements {
        match element {
            SyntaxElement::Node(node) => {
                out.push((
                    node.range().start().to_usize(),
                    node.range().end().to_usize(),
                ));
                collect_element_spans(node.children(), out);
            }
            SyntaxElement::Token(token) => {
                out.push((
                    token.range().start().to_usize(),
                    token.range().end().to_usize(),
                ));
            }
        }
    }
}

fn generated_byte_cases(count: usize, max_len: usize) -> Vec<Vec<u8>> {
    let mut seed = 0xC0FFEE_u64;
    let mut cases = Vec::with_capacity(count);
    for index in 0..count {
        let len = if max_len == 0 {
            0
        } else {
            next_u64(&mut seed) as usize % (max_len + 1)
        };
        let mut bytes = Vec::with_capacity(len + 6);
        if index % 3 == 0 {
            bytes.extend_from_slice(b"<?php ");
        }
        for _ in 0..len {
            bytes.push(generated_byte(&mut seed));
        }
        cases.push(bytes);
    }
    cases
}

fn generated_byte(seed: &mut u64) -> u8 {
    const INTERESTING: &[u8] = b"<?php \n\t$abcXYZ0123456789(){}[];,+-*/%=>!&|^~'\"#\\:\0\xFF";
    let value = next_u64(seed) as usize;
    INTERESTING[value % INTERESTING.len()]
}

fn next_u64(seed: &mut u64) -> u64 {
    *seed = seed.wrapping_mul(6364136223846793005).wrapping_add(1);
    *seed
}
