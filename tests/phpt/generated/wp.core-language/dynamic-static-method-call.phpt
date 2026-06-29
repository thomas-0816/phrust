--TEST--
Generated wp.core-language: dynamic static method call dispatch
--DESCRIPTION--
module: wp.core-language
generated timestamp: 20260629T000000Z
generator version: wp-wave3-core-language-v1
reason: factory and hook layers call static methods selected at runtime
oracle: Reference PHP 8.5.7
--FILE--
<?php
class WpWave3DynamicStatic {
    public static function render($value) {
        return "static:$value";
    }
}

$method = "render";
echo WpWave3DynamicStatic::$method("ok"), "\n";
?>
--EXPECT--
static:ok
