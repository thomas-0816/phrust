--TEST--
Generated wp.core-language: dynamic function call
--DESCRIPTION--
module: wp.core-language
generated timestamp: 20260629T000000Z
generator version: wp-wave3-core-language-v1
reason: WordPress-style dispatch through a variable function name
oracle: Reference PHP 8.5.7
--FILE--
<?php
function wp_wave3_dispatch($value) {
    echo "fn:$value\n";
    return strtoupper($value);
}

$fn = "wp_wave3_dispatch";
echo $fn("ok"), "\n";
?>
--EXPECT--
fn:ok
OK
