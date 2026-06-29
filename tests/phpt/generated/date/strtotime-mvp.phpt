--TEST--
date: strtotime controlled MVP
--DESCRIPTION--
Generated focused coverage for ISO-like dates, timestamp-like input, and selected relative modifiers.
--FILE--
<?php
date_default_timezone_set("UTC");
echo strtotime("2024-01-02 03:04:05"), "\n";
echo strtotime("@1700000000"), "\n";
echo strtotime("+1 day", 0), "\n";
echo strtotime("-1 day", 86400), "\n";
echo strtotime("next day", 0), "\n";
var_dump(strtotime("not a date"));
?>
--EXPECT--
1704164645
1700000000
86400
0
86400
bool(false)
