<?php

require __DIR__ . '/_data/external-rich-object.php';

$partial = new stdClass();
$partial->id = 1;
$partial->object_id = 99;

$query = new ExternalObjectQuery();
echo json_encode($query->relay(array($partial))), "\n";
