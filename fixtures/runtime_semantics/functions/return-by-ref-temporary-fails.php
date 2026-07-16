<?php
// runtime-semantics: expect=pass
function &bad_ref() {
    return 1;
}

$x =& bad_ref();
echo $x;
