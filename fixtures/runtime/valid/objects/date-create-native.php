<?php
$date = date_create('@0');
echo get_class($date), '|', get_debug_type($date), "\n";
echo $date->__timestamp, '|', $date->timezone, "\n";
var_dump(date_create('not-a-date'));

try {
    date_create('now', 1);
} catch (TypeError $error) {
    echo get_class($error), "\n";
}

$zones = timezone_identifiers_list(2047);
echo count($zones), '|', $zones[0], '|', $zones[8], "\n";
