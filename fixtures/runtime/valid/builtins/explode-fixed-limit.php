<?php
function explode_fixed_limit($value) {
    return explode(':', $value, 2);
}

foreach (['a:b:c', 'abc', ':a:b'] as $value) {
    echo implode('|', explode_fixed_limit($value)), "\n";
}
