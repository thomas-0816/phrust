--TEST--
pcre: preg_match and preg_match_all capture shapes
--DESCRIPTION--
Generated focused coverage for named captures, offset capture, set-order match_all, and offset-base handling.
--FILE--
<?php
$m = null;
$all = null;
preg_match('/(?P<word>[A-Z]+)-(?P<num>\d+)/', 'xx ABC-123 yy', $m, PREG_OFFSET_CAPTURE, 3);
echo $m[0][0], "|", $m[0][1], "\n";
echo $m['word'][0], "|", $m['word'][1], "|", $m[1][0], "|", $m[1][1], "\n";
echo $m['num'][0], "|", $m['num'][1], "|", $m[2][0], "|", $m[2][1], "\n";
preg_match_all('/(?P<letter>[A-Z])(?P<digit>\d)?/', 'A1 b C', $all, PREG_SET_ORDER | PREG_OFFSET_CAPTURE | PREG_UNMATCHED_AS_NULL);
echo count($all), "\n";
echo $all[0][0][0], "|", $all[0][0][1], "|", $all[0]['letter'][0], "|", $all[0]['digit'][0], "\n";
echo $all[1][0][0], "|", $all[1][0][1], "|", $all[1]['letter'][0], "|";
var_dump($all[1]['digit']);
?>
--EXPECT--
ABC-123|3
ABC|3|ABC|3
123|7|123|7
2
A1|0|A|1
C|5|C|array(2) {
  [0]=>
  NULL
  [1]=>
  int(-1)
}
