<?php

function include_variadic_local_shadow(string ...$args): string {
    return isset($args[0]) ? $args[0] : 'missing';
}
