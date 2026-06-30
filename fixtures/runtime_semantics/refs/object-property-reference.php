<?php
// runtime-semantics: expect=pass
class Box { public $p = 1; }
$box = new Box();
$alias =& $box->p;
$alias = 2;
echo $box->p;
