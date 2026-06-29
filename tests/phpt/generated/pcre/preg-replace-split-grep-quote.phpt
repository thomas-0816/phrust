--TEST--
pcre: replace, split, grep, and quote MVP behavior
--DESCRIPTION--
Generated focused coverage for replacement backrefs/count, split flags, grep, and preg_quote delimiter escaping.
--FILE--
<?php
$count = null;
var_dump(preg_replace('/(\w+) (\w+)/', '$2-$1', 'alpha beta gamma', 1, $count));
var_dump($count);
var_dump(preg_split('/(-)/', 'a-b-c', -1, PREG_SPLIT_DELIM_CAPTURE | PREG_SPLIT_NO_EMPTY));
var_dump(preg_grep('/\.php$/', ['a.php', 'b.txt', 'c.php']));
var_dump(preg_quote('a+b/c', '/'));
?>
--EXPECT--
string(16) "beta-alpha gamma"
int(1)
array(5) {
  [0]=>
  string(1) "a"
  [1]=>
  string(1) "-"
  [2]=>
  string(1) "b"
  [3]=>
  string(1) "-"
  [4]=>
  string(1) "c"
}
array(2) {
  [0]=>
  string(5) "a.php"
  [2]=>
  string(5) "c.php"
}
string(7) "a\+b\/c"
