--TEST--
date: DateTimeImmutable add() returns a new object
--DESCRIPTION--
Generated focused and 19.7 coverage for DateTimeImmutable construction, format(), getTimestamp(), and immutable add() behavior.
--FILE--
<?php
$zone = new DateTimeZone("UTC");
$date = new DateTimeImmutable("2024-01-02 00:00:00", $zone);
$changed = $date->add(new DateInterval("P1D"));
echo $date->format("Y-m-d H:i:s T U"), "\n";
echo $date->getTimestamp(), "\n";
echo $changed->format("Y-m-d H:i:s T U"), "\n";
?>
--EXPECT--
2024-01-02 00:00:00 UTC 1704153600
1704153600
2024-01-03 00:00:00 UTC 1704240000
