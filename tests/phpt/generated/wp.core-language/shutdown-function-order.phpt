--TEST--
Generated wp.core-language: shutdown function observes final globals
--DESCRIPTION--
module: wp.core-language
generated timestamp: 20260629T000000Z
generator version: wp-wave3-core-language-v1
reason: applications register request-end cleanup callbacks
oracle: Reference PHP 8.5.7
--FILE--
<?php
$shutdownValue = "global";
register_shutdown_function(function () use (&$shutdownValue) {
    echo "shutdown:$shutdownValue\n";
});

echo "body\n";
$shutdownValue = "changed";
?>
--EXPECT--
body
shutdown:changed
