--TEST--
Generated wp.core-language: dynamic property negative reads and access errors
--DESCRIPTION--
module: wp.core-language
generated timestamp: 20260629T000000Z
generator version: wp-wave3-core-language-v1
reason: dynamic property reads must warn on non-object receivers and raise PHP-like access errors
oracle: Reference PHP 8.5.7
--FILE--
<?php
set_error_handler(function($errno, $message) {
    echo "warn:$message\n";
    return true;
});
$property = "missing";
$nullValue = null;
var_dump($nullValue->$property);
$intValue = 42;
var_dump($intValue->$property);
restore_error_handler();

class WpWave3PrivateProperty {
    private $secret = "hidden";
}
$name = "secret";
try {
    var_dump((new WpWave3PrivateProperty())->$name);
} catch (Error $e) {
    echo "error:", $e->getMessage(), "\n";
}
?>
--EXPECT--
warn:Attempt to read property "missing" on null
NULL
warn:Attempt to read property "missing" on int
NULL
error:Cannot access private property WpWave3PrivateProperty::$secret
