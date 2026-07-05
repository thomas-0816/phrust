<?php
// runtime-semantics: category=known_gaps expect=known_gap known_gap=E_PHP_VM_BY_REF_VARIADIC_BINDING_GAP
// PHP reference: by-ref variadic parameters bind each tail argument as a
// reference, so writes through the collected array mutate the callers.
function tail(&...$slots): void
{
    foreach ($slots as &$slot) {
        $slot = 'tailed';
    }
}

$a = 'a';
$b = 'b';
tail($a, $b);
var_dump($a, $b);
