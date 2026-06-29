--TEST--
Generated wp.core-language: named arguments for methods and builtins
--DESCRIPTION--
module: wp.core-language
generated timestamp: 20260629T000000Z
generator version: wp-wave3-core-language-v1
reason: modern libraries use named arguments for methods and internal functions
oracle: Reference PHP 8.5.7
--FILE--
<?php
class WpWave3NamedMethod {
    public function join($first, $second = "B", $third = "C") {
        return "$first-$second-$third";
    }
}

$object = new WpWave3NamedMethod();
echo $object->join(third: "Z", first: "A"), "\n";
echo strlen(string: "hello"), "\n";
?>
--EXPECT--
A-B-Z
5
