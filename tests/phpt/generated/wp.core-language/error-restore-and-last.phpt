--TEST--
Generated wp.core-language: restore error handler and error_get_last
--DESCRIPTION--
module: wp.core-language
generated timestamp: 20260629T000000Z
generator version: wp-wave3-core-language-v1
reason: bootstrap diagnostics inspect last warning and restore temporary handlers
oracle: Reference PHP 8.5.7
--FILE--
<?php
set_error_handler(function ($errno, $errstr) {
    echo "handled:$errstr\n";
    return true;
});
trigger_error("handled-warning", E_USER_WARNING);
restore_error_handler();
trigger_error("plain-warning", E_USER_WARNING);
$last = error_get_last();
echo $last["type"], ":", $last["message"], "\n";
?>
--EXPECTF--
handled:handled-warning

Warning: plain-warning in %s on line %d
512:plain-warning
