--TEST--
date: request-local timezone state drives date()
--DESCRIPTION--
Generated focused coverage for date_default_timezone_get(), date_default_timezone_set(), date(), and separate request-local state.
--FILE--
<?php
echo date_default_timezone_get(), "\n";
var_dump(date_default_timezone_set("Europe/Berlin"));
echo date_default_timezone_get(), "\n";
echo date("Y-m-d H:i:s T O P", 0), "\n";
var_dump(@date_default_timezone_set("Mars/Base"));
echo date_default_timezone_get(), "\n";
?>
--EXPECT--
UTC
bool(true)
Europe/Berlin
1970-01-01 01:00:00 CET +0100 +01:00
bool(false)
Europe/Berlin
