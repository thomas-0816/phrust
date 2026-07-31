<?php

require __DIR__ . '/native_cross_unit_exception.inc';

for ($iteration = 0; $iteration < 10; $iteration++) {
    $result = native_external_exception($iteration === 9 ? 0 : 7);
    if ($iteration >= 8) {
        echo $result, "\n";
    }
}
