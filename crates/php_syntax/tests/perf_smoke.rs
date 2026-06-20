use php_syntax::{SyntaxElement, SyntaxNode, parse_source_file};
use std::time::Instant;

#[test]
#[ignore]
fn parser_perf_smoke_reports_baseline_metrics() {
    let cases = [
        ("small", small_file()),
        ("expression_heavy", expression_heavy_file()),
        ("class_heavy", class_heavy_file()),
        ("heredoc_string_heavy", heredoc_string_heavy_file()),
    ];

    for (name, source) in cases {
        let started = Instant::now();
        let parse = parse_source_file(&source);
        let elapsed = started.elapsed();
        let counts = count_tree(parse.root());

        assert_eq!(parse.reconstructed_text(), source, "{name}");
        println!(
            "parser_perf name={name} bytes={} elapsed_us={} nodes={} tokens={} diagnostics={}",
            source.len(),
            elapsed.as_micros(),
            counts.nodes,
            counts.tokens,
            parse.diagnostics().len()
        );
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
struct TreeCounts {
    nodes: usize,
    tokens: usize,
}

fn count_tree(root: &SyntaxNode) -> TreeCounts {
    let mut counts = TreeCounts {
        nodes: 1,
        tokens: 0,
    };
    count_children(root.children(), &mut counts);
    counts
}

fn count_children(children: &[SyntaxElement], counts: &mut TreeCounts) {
    for child in children {
        match child {
            SyntaxElement::Node(node) => {
                counts.nodes += 1;
                count_children(node.children(), counts);
            }
            SyntaxElement::Token(_token) => counts.tokens += 1,
        }
    }
}

fn small_file() -> String {
    "<?php echo 1;\n".to_owned()
}

fn expression_heavy_file() -> String {
    let mut source = String::from("<?php\n$result = 0;\n");
    for index in 0..120 {
        source.push_str(&format!("$result = (($result + {index}) * 3) ** 2 ?? 0;\n"));
    }
    source.push_str("echo $result;\n");
    source
}

fn class_heavy_file() -> String {
    let mut source = String::from("<?php\n");
    for class_index in 0..40 {
        source.push_str(&format!("class C{class_index} {{\n"));
        source.push_str("    public const match = 1;\n");
        source.push_str("    public function match(int $x): int {\n");
        source.push_str("        return $x + self::match;\n");
        source.push_str("    }\n");
        source.push_str("}\n");
    }
    source
}

fn heredoc_string_heavy_file() -> String {
    let mut source = String::from("<?php\n");
    for index in 0..30 {
        source.push_str(&format!("$text{index} = <<<TXT\n"));
        source.push_str("hello {$name}\n");
        source.push_str("line two\n");
        source.push_str("TXT;\n");
        source.push_str(&format!("echo \"value {index}: {{$text{index}}}\\n\";\n"));
    }
    source
}
