--TEST--
pcre: preg_replace_callback invalid callable throws TypeError
--DESCRIPTION--
Generated focused coverage for invalid callback dispatch.
--FILE--
<?php
try {
    preg_replace_callback('/x/', 'missing_callback', 'x');
} catch (TypeError $e) {
    echo $e->getMessage(), "\n";
}
?>
--EXPECT--
preg_replace_callback(): Argument #2 ($callback) must be a valid callback, function "missing_callback" not found or invalid function name
