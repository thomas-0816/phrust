--TEST--
standard.url-html: URL encoding and decoding helpers
--FILE--
<?php
$input = "a b~!*()+'\"&=";
var_dump(urlencode($input));
var_dump(urldecode(urlencode($input)));
var_dump(rawurlencode($input));
var_dump(rawurldecode(rawurlencode($input)));
var_dump(urldecode('a+b%7E'));
var_dump(rawurldecode('a+b%7E'));
?>
--EXPECT--
string(33) "a+b%7E%21%2A%28%29%2B%27%22%26%3D"
string(13) "a b~!*()+'"&="
string(33) "a%20b~%21%2A%28%29%2B%27%22%26%3D"
string(13) "a b~!*()+'"&="
string(4) "a b~"
string(4) "a+b~"
