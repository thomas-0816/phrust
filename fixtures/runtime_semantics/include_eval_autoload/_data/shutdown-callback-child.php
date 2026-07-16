<?php

register_shutdown_function(static function (string $message): void {
    echo $message, "\n";
}, 'shutdown');
