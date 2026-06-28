--TEST--
Tokenizer token_get_all exposes names, text, and lines
--DESCRIPTION--
contract source: tokenizer frontend smoke
generator version: phpt-tokenizer-curated-v1
reason: in-scope token_get_all surface backed by php_lexer
--FILE--
<?php
$tokens = token_get_all("<?php\n echo \$name + 1;\n");
foreach ($tokens as $token) {
    if (is_array($token)) {
        echo token_name($token[0]), "|", str_replace("\n", "\\n", $token[1]), "|", $token[2], "\n";
    } else {
        echo $token, "\n";
    }
}
--EXPECT--
T_OPEN_TAG|<?php\n|1
T_WHITESPACE| |2
T_ECHO|echo|2
T_WHITESPACE| |2
T_VARIABLE|$name|2
T_WHITESPACE| |2
+
T_WHITESPACE| |2
T_LNUMBER|1|2
;
T_WHITESPACE|\n|2
