--TEST--
date: DateTime construction and format MVP
--DESCRIPTION--
Generated focused coverage for DateTime construction, default timezone, format(), getTimestamp(), and date_format().
--FILE--
<?php
date_default_timezone_set("Europe/Berlin");
$date = new DateTime("2024-01-02 03:04:05");
echo $date->format("Y-m-d H:i:s T U"), "\n";
echo $date->getTimestamp(), "\n";
echo date_format($date, "c"), "\n";
$now = new DateTime();
if ($now->getTimestamp() > 0) {
    echo "now-ok\n";
} else {
    echo "now-bad\n";
}
?>
--EXPECT--
2024-01-02 03:04:05 CET 1704161045
1704161045
2024-01-02T03:04:05+01:00
now-ok
