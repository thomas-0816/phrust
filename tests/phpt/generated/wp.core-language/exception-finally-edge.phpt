--TEST--
Generated wp.core-language: exception and finally edge
--DESCRIPTION--
module: wp.core-language
generated timestamp: 20260629T000000Z
generator version: wp-wave3-core-language-v1
reason: bootstrap cleanup depends on finally running before catch dispatch
oracle: Reference PHP 8.5.7
--FILE--
<?php
function wp_wave3_finally() {
    try {
        echo "try\n";
        throw new Exception("boom");
    } finally {
        echo "finally\n";
    }
}

try {
    wp_wave3_finally();
} catch (Exception $e) {
    echo "catch:", $e->getMessage(), "\n";
}
?>
--EXPECT--
try
finally
catch:boom
