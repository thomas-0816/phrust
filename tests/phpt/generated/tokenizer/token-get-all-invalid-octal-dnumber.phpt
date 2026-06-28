--TEST--
Tokenizer token_get_all classifies overflowing invalid octal as T_DNUMBER
--DESCRIPTION--
original php-src path: ext/tokenizer/tests/invalid_octal_dnumber.phpt
original source hash: 5572fc624e88457d37491b9037a3edf2091c3fde0f05cba7517fe57a9d6bdf6b
generator version: phpt-tokenizer-curated-v1
reason: smallest in-scope reference tokenizer overflow case
--FILE--
<?php
echo token_name(token_get_all('<?php 0177777777777777777777787')[1][0]), "\n";
--EXPECT--
T_DNUMBER
