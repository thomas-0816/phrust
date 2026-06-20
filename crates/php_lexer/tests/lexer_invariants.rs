use php_lexer::{LexerConfig, TokenKind, lex_all};

#[test]
fn lexer_invariants_cover_problem_inputs() {
    let inputs = [
        String::new(),
        "<?php".to_owned(),
        "<?php echo 'unterminated".to_owned(),
        "<?php echo \"unterminated $name".to_owned(),
        "<?php /* unterminated".to_owned(),
        "<?php <<<TXT\nbody\n".to_owned(),
        "<?php $\n\\\n".to_owned(),
        "<?php \u{0007}".to_owned(),
        "<?php café();".to_owned(),
        "<?php $x = 1__2; $y = 0x; $z = 1e;".to_owned(),
        "<?php echo \"${name} {$object->property} ${items[0]}\";".to_owned(),
        "<?php ".repeat(512),
    ];

    for input in inputs {
        let result = std::panic::catch_unwind(|| {
            lex_all(
                &input,
                LexerConfig {
                    emit_eof: true,
                    ..LexerConfig::default()
                },
            )
        })
        .unwrap_or_else(|_| panic!("lex_all panicked for input {input:?}"));

        let mut previous_end = 0;
        for token in &result.tokens {
            let start = token.range.start().to_usize();
            let end = token.range.end().to_usize();
            assert!(start <= end, "token range is ordered for {input:?}");
            assert!(
                end <= input.len(),
                "token range is within source for {input:?}"
            );
            assert!(
                start >= previous_end,
                "tokens do not overlap for {input:?}: previous end {previous_end}, start {start}"
            );
            if token.kind != TokenKind::Eof {
                assert!(end > start, "non-EOF token makes progress for {input:?}");
            }
            previous_end = end;
        }
    }
}
