--TEST--
Generated wp.core-language: argument unpacking
--DESCRIPTION--
module: wp.core-language
generated timestamp: 20260629T000000Z
generator version: wp-wave3-core-language-v1
reason: dispatch helpers pass collected arguments with ... unpacking
oracle: Reference PHP 8.5.7
--FILE--
<?php
function wp_wave3_unpack($a, $b, $c) {
    echo "$a|$b|$c\n";
}

wp_wave3_unpack(...[1, 2, 3]);
?>
--EXPECT--
1|2|3
