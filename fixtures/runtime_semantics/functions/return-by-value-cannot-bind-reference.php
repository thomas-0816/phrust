<?php
// runtime-semantics: expect=pass
function value_only() {
    $x = 1;
    return $x;
}

$r =& value_only();
echo $r;
