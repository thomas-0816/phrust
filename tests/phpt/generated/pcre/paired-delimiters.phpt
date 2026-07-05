--TEST--
pcre: paired pattern delimiters are parsed with nesting
--DESCRIPTION--
Generated focused coverage for PHP paired delimiters and nested delimiter bytes
inside PCRE groups, quantifiers, character classes, and named captures.
--FILE--
<?php
var_dump(preg_match('{a{1,2}}', 'aa'));
var_dump(preg_match('{[a-z]{1,2}}i', 'AZ'));

$matches = null;
var_dump(preg_match('(a(b)c)', 'abc', $matches));
var_dump($matches[1]);

var_dump(preg_match('[[a-z]]', 'm'));

$matches = null;
var_dump(preg_match('<(?<word>[a-z]+)>', 'abc', $matches));
var_dump($matches['word']);
?>
--EXPECT--
int(1)
int(1)
int(1)
string(1) "b"
int(1)
int(1)
string(3) "abc"
