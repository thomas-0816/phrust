<?php

$counter = 0;

function bump_cached_global(): int
{
    global $counter;
    return ++$counter;
}

for ($index = 0; $index < 1000; ++$index) {
    bump_cached_global();
}
echo $counter, '|';

$alias =& $GLOBALS['counter'];
$alias = 2000;
echo bump_cached_global(), '|';

unset($GLOBALS['counter']);
$alias = 3000;
echo isset($counter) ? 'present' : 'missing', '|';

$counter = 10;
echo bump_cached_global(), "\n";
