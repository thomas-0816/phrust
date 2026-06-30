<?php
// runtime-semantics: category=globals expect=known_gap known_gap=E_PHP_RUNTIME_GLOBALS_ALIAS_MATRIX
$name = "x";
$$name = 1;
function read_dynamic_global($name) {
    global $$name;
    echo $x;
}
read_dynamic_global("x");
