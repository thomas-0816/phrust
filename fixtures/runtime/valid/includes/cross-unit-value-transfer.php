<?php
// runtime-fixture: kind=valid
class CrossUnitTransferObject {}

require __DIR__ . '/lib/cross-unit-value-transfer-target.php';

$input = array('outside' => 1);
$object = new CrossUnitTransferObject();
$result = cross_unit_transfer_values($input, $object, 'text', 41);
$returned = $result[0];
$returned['caller'] = 3;

echo $input['outside'], '|', isset($input['inside']) ? '1' : '0', '|';
echo $returned['inside'], '|', $returned['caller'], '|';
echo $result[1] === $object ? 'same' : 'different', '|';
echo $result[2], '|', $result[3], '|', $result[4], "\n";
