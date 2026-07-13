<?php
// Regression: ArrayObject(ARRAY_AS_PROPS) property reads and writes must
// route through the container's array storage on the dense path too. The
// dense helpers previously stored such writes as (deprecated) dynamic
// properties and missed them on read.
function dense_assign($o) { $o->x = 5; return $o->x; }
function rich_assign($o) { try { $o->y = 7; } finally {} return $o->y; }
$a = new ArrayObject([], ArrayObject::ARRAY_AS_PROPS);
var_dump(dense_assign($a));
var_dump(rich_assign($a));
var_dump($a['x'] ?? null);
var_dump($a['y'] ?? null);
var_dump($a->getArrayCopy());
