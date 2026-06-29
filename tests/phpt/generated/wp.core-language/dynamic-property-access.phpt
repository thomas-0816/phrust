--TEST--
Generated wp.core-language: dynamic property access
--DESCRIPTION--
module: wp.core-language
generated timestamp: 20260629T000000Z
generator version: wp-wave3-core-language-v1
reason: application option and metadata objects use variable property names
oracle: Reference PHP 8.5.7
--FILE--
<?php
$props = new stdClass();
$prop = "option_name";
$props->$prop = "siteurl";
echo $props->option_name, "|", $props->$prop, "\n";
$slot = "items";
$props->$slot = [];
$props->$slot["first"] = "stored";
var_dump(isset($props->$slot));
var_dump(empty($props->$slot));
var_dump(isset($props->$slot["first"]));
var_dump(empty($props->$slot["missing"]));
echo $props->$slot["first"], "\n";
?>
--EXPECT--
siteurl|siteurl
bool(true)
bool(false)
bool(true)
bool(true)
stored
