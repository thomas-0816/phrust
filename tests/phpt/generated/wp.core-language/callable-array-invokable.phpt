--TEST--
Generated wp.core-language: callable arrays and invokable objects
--DESCRIPTION--
module: wp.core-language
generated timestamp: 20260629T000000Z
generator version: wp-wave3-core-language-v1
reason: dispatcher callbacks use callable arrays, string callables, and invokable objects
oracle: Reference PHP 8.5.7
--FILE--
<?php
function wp_wave3_callable_string($value) {
    return "string:$value";
}

class WpWave3CallableTarget {
    public function objectCall($value) {
        return "array-object:$value";
    }

    public static function staticCall($value) {
        return "array-static:$value";
    }

    public function __invoke($value) {
        return "invoke:$value";
    }
}

$string = "wp_wave3_callable_string";
$object = new WpWave3CallableTarget();
$objectCallable = [$object, "objectCall"];
$staticCallable = ["WpWave3CallableTarget", "staticCall"];
$invokable = $object;

echo $string("s"), "\n";
echo $objectCallable("o"), "\n";
echo $staticCallable("t"), "\n";
echo $invokable("i"), "\n";
?>
--EXPECT--
string:s
array-object:o
array-static:t
invoke:i
