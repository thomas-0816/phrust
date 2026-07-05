--TEST--
pcre: preg_replace_callback flags shape callback match arrays
--DESCRIPTION--
Generated focused coverage for PHP 8.5 callback flags in
preg_replace_callback and preg_replace_callback_array.
--FILE--
<?php
var_dump(preg_replace_callback('/./', function ($matches) {
    var_dump($matches);
    return $matches[0][0];
}, 'ab', -1, $count, PREG_OFFSET_CAPTURE));
var_dump($count);

var_dump(preg_replace_callback_array([
    '/(a)|(b)/' => function ($matches) {
        var_dump($matches);
        return $matches[0];
    },
], 'ab', -1, $count, PREG_UNMATCHED_AS_NULL));
var_dump($count);
?>
--EXPECT--
array(1) {
  [0]=>
  array(2) {
    [0]=>
    string(1) "a"
    [1]=>
    int(0)
  }
}
array(1) {
  [0]=>
  array(2) {
    [0]=>
    string(1) "b"
    [1]=>
    int(1)
  }
}
string(2) "ab"
int(2)
array(3) {
  [0]=>
  string(1) "a"
  [1]=>
  string(1) "a"
  [2]=>
  NULL
}
array(3) {
  [0]=>
  string(1) "b"
  [1]=>
  NULL
  [2]=>
  string(1) "b"
}
string(2) "ab"
int(2)
