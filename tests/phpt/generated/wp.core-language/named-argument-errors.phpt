--TEST--
Generated wp.core-language: duplicate and unknown named argument errors
--DESCRIPTION--
module: wp.core-language
generated timestamp: 20260629T000000Z
generator version: wp-wave3-core-language-v1
reason: dynamic dispatch must raise PHP-like errors for invalid named arguments
oracle: Reference PHP 8.5.7
--FILE--
<?php
function wp_wave3_named_errors($first) {
    return $first;
}

try {
    wp_wave3_named_errors(first: "a", first: "b");
} catch (Throwable $e) {
    echo get_class($e), ":", $e->getMessage(), "\n";
}

try {
    wp_wave3_named_errors(missing: "x");
} catch (Throwable $e) {
    echo get_class($e), ":", $e->getMessage(), "\n";
}
?>
--EXPECT--
Error:Named parameter $first overwrites previous argument
Error:Unknown named parameter $missing
