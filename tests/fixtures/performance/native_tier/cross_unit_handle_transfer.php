<?php
require __DIR__ . '/cross_unit_handle_transfer_target.php';

class PerfCrossUnitObject {}

$object = new PerfCrossUnitObject();
$text = str_repeat('x', 32);
$matches = 0;
for ($i = 0; $i < 1000; $i++) {
    $returned = perf_cross_unit_identity($object, $text, $i);
    if ($returned === $object) {
        $matches++;
    }
}
echo $matches, "\n";
