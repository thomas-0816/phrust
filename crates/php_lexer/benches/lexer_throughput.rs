use php_lexer::{LexerConfig, lex_all};
use std::hint::black_box;
use std::time::Instant;

fn main() {
    let cases = [
        (
            "inline-html",
            "<html><body>Hello</body></html>\n".repeat(20_000),
        ),
        (
            "simple-statements",
            "<?php\n$x = 1; $y = $x + 2; echo $y;\n".repeat(20_000),
        ),
        (
            "strings-interpolation",
            "<?php\n$name = 'world'; echo \"hello $name {$object->field}\";\n".repeat(12_000),
        ),
    ];

    for (name, source) in cases {
        let started = Instant::now();
        let mut token_count = 0usize;
        for _ in 0..20 {
            let result = lex_all(black_box(&source), LexerConfig::default());
            token_count += result.tokens.len();
            black_box(result);
        }
        let elapsed = started.elapsed();
        let mib = (source.len() * 20) as f64 / (1024.0 * 1024.0);
        let throughput = mib / elapsed.as_secs_f64();
        println!("{name}: {throughput:.2} MiB/s over {mib:.2} MiB ({token_count} tokens)");
    }
}
