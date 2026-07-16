<?php
// runtime-semantics: expect=pass regression_category=objects reference_behavior=stdout:child regression_case=cross-unit-virtual-method-dispatch

require __DIR__ . '/../_data/cross-unit-virtual-parent.php';
require __DIR__ . '/../_data/cross-unit-virtual-child.php';

$object = new CrossUnitVirtualChild();
echo $object->run(), "\n";
