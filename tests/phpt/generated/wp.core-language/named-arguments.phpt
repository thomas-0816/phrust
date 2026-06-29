--TEST--
Generated wp.core-language: named arguments
--DESCRIPTION--
module: wp.core-language
generated timestamp: 20260629T000000Z
generator version: wp-wave3-core-language-v1
reason: modern framework callbacks use user-function named arguments
oracle: Reference PHP 8.5.7
--FILE--
<?php
function wp_wave3_named($first, $second = "b", $third = "c") {
    echo "$first-$second-$third\n";
}

wp_wave3_named(third: "C", first: "A");
?>
--EXPECT--
A-b-C
