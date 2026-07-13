<?php
// Undefined function calls throw catchable Error with the reference wording
// (minimized from Zend/tests error-message PHPTs).
try {
    nonexistent_fn();
} catch (Error $e) {
    echo get_class($e), ": ", $e->getMessage(), "\n";
}
try {
    $f = 'no_such_fn';
    $f();
} catch (Error $e) {
    echo "dyn: ", $e->getMessage(), "\n";
}
function wrapper() { missing_inner(); }
try {
    wrapper();
} catch (Error $e) {
    echo "inner: ", $e->getMessage(), "\n";
}
echo "done\n";
