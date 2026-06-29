--TEST--
date: DateTimeZone focused registry MVP
--DESCRIPTION--
Generated focused coverage for DateTimeZone construction, getName(), timezone_name_get(), timezone_open(), and DateTime constructor timezone argument.
--FILE--
<?php
$zone = new DateTimeZone("UTC");
echo $zone->getName(), "\n";
echo timezone_name_get($zone), "\n";
echo timezone_open("Europe/Berlin")->getName(), "\n";
$date = new DateTime("1970-01-01 00:00:00", new DateTimeZone("Asia/Tokyo"));
echo $date->format("Y-m-d H:i:s T O"), "\n";
var_dump(@timezone_open("Mars/Base"));
?>
--EXPECT--
UTC
UTC
Europe/Berlin
1970-01-01 00:00:00 JST +0900
bool(false)
