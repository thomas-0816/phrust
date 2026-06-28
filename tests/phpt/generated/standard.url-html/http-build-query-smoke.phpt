--TEST--
standard.url-html: http_build_query array MVP
--FILE--
<?php
$data = ["foo" => "bar", "space key" => "a b", "quote" => "'\"", "truth" => true, "falsey" => false, "skip" => null, "nested" => ["x" => 1, "y" => "z"]];
var_dump(http_build_query($data));
?>
--EXPECT--
string(83) "foo=bar&space+key=a+b&quote=%27%22&truth=1&falsey=0&nested%5Bx%5D=1&nested%5By%5D=z"
