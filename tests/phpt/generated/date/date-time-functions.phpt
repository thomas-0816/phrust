--TEST--
date: time, microtime, date and gmdate focused output
--DESCRIPTION--
Generated focused coverage for time(), microtime(), date(), gmdate(), and selected deterministic format characters.
--FILE--
<?php
if (time() > 0) {
    echo "time-ok\n";
} else {
    echo "time-bad\n";
}
$micro = microtime();
if (is_string($micro) && preg_match('/^0\.\d+ \d+$/', $micro)) {
    echo "micro-string-ok\n";
} else {
    echo "micro-string-bad\n";
}
if (microtime(true) > 0.0) {
    echo "micro-float-ok\n";
} else {
    echo "micro-float-bad\n";
}
date_default_timezone_set("Europe/Berlin");
echo date("Y-y-m-n-d-j H-G-i-s U c O P T", 0), "\n";
echo gmdate("Y-m-d H:i:s T O P", 0), "\n";
?>
--EXPECT--
time-ok
micro-string-ok
micro-float-ok
1970-70-01-1-01-1 01-1-00-00 0 1970-01-01T01:00:00+01:00 +0100 +01:00 CET
1970-01-01 00:00:00 GMT +0000 +00:00
