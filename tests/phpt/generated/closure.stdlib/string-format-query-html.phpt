--TEST--
closure.stdlib: string formatting, query, and HTML helpers
--DESCRIPTION--
Generated closure stdlib coverage for printf/sprintf, parse_str,
http_build_query, urlencode/rawurlencode, and default HTML escaping.
--FILE--
<?php
printf("%04d %.2f %s\n", 7, 1.5, "ok");
var_dump(sprintf("%s:%X:%o", "id", 255, 9));
$query = http_build_query(["a" => 1, "b" => ["x" => "y z"]]);
var_dump($query);
parse_str($query, $out);
var_dump($out);
var_dump(urlencode("a b+c"));
var_dump(rawurlencode("a b+c"));
$html = "<a href='x&y'>\"Hi\"</a>";
var_dump(htmlspecialchars($html));
var_dump(htmlspecialchars_decode(htmlspecialchars($html)));
?>
--EXPECT--
0007 1.50 ok
string(8) "id:FF:11"
string(16) "a=1&b%5Bx%5D=y+z"
array(2) {
  ["a"]=>
  string(1) "1"
  ["b"]=>
  array(1) {
    ["x"]=>
    string(3) "y z"
  }
}
string(7) "a+b%2Bc"
string(9) "a%20b%2Bc"
string(58) "&lt;a href=&#039;x&amp;y&#039;&gt;&quot;Hi&quot;&lt;/a&gt;"
string(22) "<a href='x&y'>"Hi"</a>"
