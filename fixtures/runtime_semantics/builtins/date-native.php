<?php
// runtime-semantics: category=builtins expect=pass

date_default_timezone_set('UTC');

var_dump(checkdate(2, 29, 2024));
var_dump(checkdate(2, 29, 2023));
echo date('Y-m-d H:i:s', 0), "\n";
echo gmdate('Y-m-d H:i:s', 0), "\n";
echo strtotime('+1 day', 0), "\n";
echo mktime(0, 0, 0, 1, 1, 1970), "\n";
echo gmmktime(0, 0, 0, 1, 1, 1970), "\n";

$timezones = timezone_identifiers_list();
var_dump(in_array('UTC', $timezones, true));
var_dump(in_array('Europe/Berlin', $timezones, true));
