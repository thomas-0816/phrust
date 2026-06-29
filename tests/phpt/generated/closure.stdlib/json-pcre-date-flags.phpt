--TEST--
closure.stdlib: JSON, PCRE, and Date flag helpers
--DESCRIPTION--
Generated closure stdlib coverage for selected JSON flags, PCRE captures and
replacement, and deterministic Date/Time formatting.
--FILE--
<?php
echo json_encode(["a" => "<tag>", "b" => 1], JSON_HEX_TAG), "\n";
var_dump(json_decode('{"n":2}', true));
try {
    json_decode("{", true, 512, JSON_THROW_ON_ERROR);
} catch (JsonException $e) {
    echo "json-exception\n";
}

var_dump(preg_match('/(?P<word>[a-z]+)(\d+)/', 'abc123', $m));
var_dump($m["word"], $m[2]);
echo preg_replace('/\d+/', '#', 'abc123'), "\n";

date_default_timezone_set("UTC");
echo gmdate("Y-m-d H:i:s T O", 0), "\n";
var_dump(strtotime("1970-01-02 00:00:00 UTC"));
?>
--EXPECT--
{"a":"\u003Ctag\u003E","b":1}
array(1) {
  ["n"]=>
  int(2)
}
json-exception
int(1)
string(3) "abc"
string(3) "123"
abc#
1970-01-01 00:00:00 GMT +0000
int(86400)
