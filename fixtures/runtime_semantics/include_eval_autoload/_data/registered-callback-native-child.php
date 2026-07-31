<?php

function registered_child_loader(string $class): void
{
    echo "child:", $class, "\n";
}

spl_autoload_unregister('registered_parent_drop');
spl_autoload_register('registered_child_loader', true, true);
register_shutdown_function(static function (string $message, int $number): void {
    echo $message, ':', $number, "\n";
}, 'shutdown', 7);
