--TEST--
Tokenizer token_name covers PHP 8.5 token names
--DESCRIPTION--
contract source: tokenizer PHP 8.5 token-name smoke
generator version: phpt-tokenizer-curated-v1
reason: in-scope token_name and token constants surface
--FILE--
<?php
foreach ([T_PUBLIC_SET, T_PROTECTED_SET, T_PRIVATE_SET, T_PROPERTY_C, T_PIPE] as $id) {
    echo token_name($id), "\n";
}
echo token_name(-1), "\n";
--EXPECT--
T_PUBLIC_SET
T_PROTECTED_SET
T_PRIVATE_SET
T_PROPERTY_C
T_PIPE
UNKNOWN
