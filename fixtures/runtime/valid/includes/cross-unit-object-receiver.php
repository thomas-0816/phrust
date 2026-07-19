<?php
require __DIR__ . '/lib/cross-unit-receiver-target.php';
require __DIR__ . '/lib/cross-unit-receiver-owner.php';

$owner = new CrossUnitReceiverOwner();
echo $owner->add_values( array( 'color' => 'red' ) ) ? "object\n" : "wrong\n";
