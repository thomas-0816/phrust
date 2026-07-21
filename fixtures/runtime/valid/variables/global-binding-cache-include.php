<?php

$included_counter = 0;
require __DIR__ . '/global-binding-cache-include-target.php';

for ($index = 0; $index < 1000; ++$index) {
    bump_included_cached_global();
}

echo $included_counter, '|';

require __DIR__ . '/global-binding-cache-include-rebind.php';
echo bump_included_cached_global(), "\n";
