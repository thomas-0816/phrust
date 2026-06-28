--TEST--
Tokenizer TOKEN_PARSE is accepted for lexer-supported tokenization
--DESCRIPTION--
contract source: tokenizer TOKEN_PARSE supported subset smoke
generator version: phpt-tokenizer-curated-v1
reason: TOKEN_PARSE coverage limited to current php_lexer-supported behavior
--FILE--
<?php
$tokens = token_get_all('<?php echo 1; ?>tail', TOKEN_PARSE);
foreach ($tokens as $token) {
    echo is_array($token) ? token_name($token[0]) : $token;
    echo "\n";
}
--EXPECT--
T_OPEN_TAG
T_ECHO
T_WHITESPACE
T_LNUMBER
;
T_WHITESPACE
T_CLOSE_TAG
T_INLINE_HTML
