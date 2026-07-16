<?php

function static_local_after_require() {
    static $suffixes;

    if (null === $suffixes) {
        require __DIR__ . '/static-local-after-require-version.php';
        $suffixes = array('suffix' => '.min');
    }

    return $suffixes['suffix'];
}
