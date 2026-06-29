--TEST--
date: DateInterval focused MVP
--DESCRIPTION--
Generated focused coverage for DateInterval ISO subset parsing, properties, format(), date_interval_format(), and DateTime add/sub integration.
--FILE--
<?php
$interval = new DateInterval("P1DT2H3M4S");
echo $interval->y, "|", $interval->m, "|", $interval->d, "|", $interval->h, "|", $interval->i, "|", $interval->s, "|", $interval->invert, "\n";
echo $interval->format("%R %d %h %i %s"), "\n";
echo date_interval_format($interval, "%d %h %i %s"), "\n";
$date = new DateTime("2024-01-02 00:00:00", new DateTimeZone("UTC"));
echo $date->add($interval)->format("Y-m-d H:i:s T"), "\n";
echo $date->sub(new DateInterval("P1D"))->format("Y-m-d H:i:s T"), "\n";
?>
--EXPECT--
0|0|1|2|3|4|0
+ 1 2 3 4
1 2 3 4
2024-01-03 02:03:04 UTC
2024-01-02 02:03:04 UTC
