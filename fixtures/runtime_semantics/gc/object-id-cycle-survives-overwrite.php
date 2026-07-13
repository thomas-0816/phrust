<?php
// runtime-semantics: category=gc expect=pass
// Cycle members stay alive (and keep their ids) across an overwrite of
// their container: reference PHP defers cyclic garbage to the collector,
// and the engine must not recycle their ids at the store either.
class C { public $peer; }

$a = new C(); $b = new C();
$a->peer = $b; $b->peer = $a;
$ida = spl_object_id($a);
$holder = ['pair' => [$a, $b]];
unset($a, $b);
$holder = null;
$probe = new C();
var_dump(spl_object_id($probe) === $ida);
