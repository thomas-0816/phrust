<?php
// runtime-semantics: category=arrays expect=pass

var_dump(array_intersect([['first']], [['second']]));
var_dump(array_intersect_assoc(['key' => ['first']], ['key' => ['second']]));
