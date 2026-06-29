--TEST--
Generated wp.core-language: custom error handler catches warning
--DESCRIPTION--
module: wp.core-language
generated timestamp: 20260629T000000Z
generator version: wp-wave3-core-language-v1
reason: bootstrap code installs warning handlers and restores them
oracle: Reference PHP 8.5.7
--FILE--
<?php
set_error_handler(function ($errno, $errstr) {
    echo "handled:$errno\n";
    return true;
});
echo $wp_wave3_missing_variable;
restore_error_handler();
echo "handler-after\n";
?>
--EXPECT--
handled:2
handler-after
