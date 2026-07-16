<?php

register_shutdown_function(static function (): void {
    throw new RuntimeException('shutdown boom');
});

echo "body\n";
