<?php
class C { final public int $x = 1; }
class D extends C {}
$d = new D();
var_dump($d->x);
echo "ok\n";
