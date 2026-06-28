--TEST--
Tokenizer token_get_all exposes PHP 8.5 lexer tokens
--DESCRIPTION--
contract source: tokenizer PHP 8.5 lexer token smoke
generator version: phpt-tokenizer-curated-v1
reason: in-scope PHP 8.5 token names emitted by php_lexer
--FILE--
<?php
$code = '<?php class C { public(set) string $name { get => __PROPERTY__; } } $value |> process(...$args);';
foreach (token_get_all($code) as $token) {
    if (is_array($token)) {
        $name = token_name($token[0]);
        if (in_array($name, ['T_PUBLIC_SET', 'T_PROPERTY_C', 'T_PIPE', 'T_ELLIPSIS', 'T_VARIABLE'], true)) {
            echo $name, "|", $token[1], "\n";
        }
    }
}
--EXPECT--
T_PUBLIC_SET|public(set)
T_VARIABLE|$name
T_PROPERTY_C|__PROPERTY__
T_VARIABLE|$value
T_PIPE||>
T_ELLIPSIS|...
T_VARIABLE|$args
