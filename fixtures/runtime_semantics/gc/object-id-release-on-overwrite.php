<?php
// runtime-semantics: category=gc expect=pass
// Pins spl_object_id release timing around container overwrites: an
// exclusively-held graph frees at the overwrite, a rooted object survives,
// and a mixed graph (shared plain array beside an exclusive object) frees
// its exclusive member — identical whether the engine releases eagerly or
// at the natural drop that immediately follows the overwrite.
class P {}

$holder = ['p' => new P()];
$id1 = spl_object_id($holder['p']);
$holder = null;
$next = new P();
var_dump(spl_object_id($next) === $id1);
unset($next);

$obj = new P();
$keep = [$obj];
$holder2 = ['k' => $keep];
$id2 = spl_object_id($obj);
$holder2 = null;
$fresh = new P();
var_dump(spl_object_id($fresh) === $id2);
var_dump(spl_object_id($obj) === $id2);
unset($fresh, $obj, $keep);

$keep2 = [1, 2, 3];
$holder3 = ['a' => $keep2, 'b' => new P()];
$id3 = spl_object_id($holder3['b']);
$holder3 = null;
$after = new P();
var_dump(spl_object_id($after) === $id3);
