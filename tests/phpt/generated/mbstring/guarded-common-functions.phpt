--TEST--
mbstring: common functions are guarded while extension is unavailable
--DESCRIPTION--
Focused mbstring stub coverage that makes unavailable functions visible without pretending mbstring is loaded.
--FILE--
<?php
foreach ([
    "mb_strlen",
    "mb_substr",
    "mb_strtolower",
    "mb_strtoupper",
    "mb_detect_encoding",
] as $function) {
    echo $function, function_exists($function) ? " available\n" : " unavailable\n";
}
?>
--EXPECT--
mb_strlen unavailable
mb_substr unavailable
mb_strtolower unavailable
mb_strtoupper unavailable
mb_detect_encoding unavailable
