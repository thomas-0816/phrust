--TEST--
pcre: preg_replace_callback dispatches named functions and closures
--DESCRIPTION--
Generated focused coverage for real VM callable dispatch through preg_replace_callback.
--FILE--
<?php
function pcre_wrap($m) {
    return '[' . $m[0] . ']';
}
$count = null;
var_dump(preg_replace_callback('/foo/', 'pcre_wrap', 'foo bar foo', -1, $count));
var_dump($count);
$count = null;
var_dump(preg_replace_callback('/(?P<word>bar)/', function ($m) {
    return strtoupper($m['word']);
}, 'foo bar', 1, $count));
var_dump($count);
?>
--EXPECT--
string(15) "[foo] bar [foo]"
int(2)
string(7) "foo BAR"
int(1)
