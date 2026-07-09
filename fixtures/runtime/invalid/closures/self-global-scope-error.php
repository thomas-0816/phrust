<?php
// Reference-confirmed (PHP 8.5.7): invoking an unscoped closure that touches
// self:: throws a catchable Error at call time, not a compile diagnostic.
$c = function () { return self::class; };
echo $c();
