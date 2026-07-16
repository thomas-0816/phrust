<?php

function static_local_array_dim($key) {
    static $cache = array();

    if (isset($cache[$key])) {
        return $cache[$key];
    }

    $cache[$key] = file_exists(__DIR__ . '/missing-theme.json');
    return $cache[$key];
}

var_dump(static_local_array_dim('theme'));
var_dump(static_local_array_dim('theme'));
