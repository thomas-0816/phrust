--TEST--
Generated wp.core-language: array spread and unpack
--DESCRIPTION--
module: wp.core-language
generated timestamp: 20260629T000000Z
generator version: wp-wave3-core-language-v1
reason: configuration arrays use spread/unpack with integer and string keys
oracle: Reference PHP 8.5.7
--FILE--
<?php
$spread = ["start", ...[1, 2], "name" => "old", ...["name" => "new", "tail" => "end"]];
foreach ($spread as $key => $value) {
    echo "$key:$value ";
}
echo "\n";
?>
--EXPECT--
0:start 1:1 2:2 name:new tail:end
