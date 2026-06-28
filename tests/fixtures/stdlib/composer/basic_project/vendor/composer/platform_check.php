<?php

$issues = array();

if (!defined('PHP_VERSION_ID')) {
    $issues[] = 'missing PHP_VERSION_ID';
}

if (PHP_VERSION_ID < 80500) {
    $issues[] = 'php version';
}

if (!version_compare(PHP_VERSION, '8.5.0', '>=')) {
    $issues[] = 'version compare';
}

if (!extension_loaded('json')) {
    $issues[] = 'json extension';
}

if (!function_exists('json_encode')) {
    $issues[] = 'json_encode';
}

if (!class_exists('JsonException', false)) {
    $issues[] = 'JsonException';
}

if (ini_get('default_charset') !== 'UTF-8') {
    $issues[] = 'default_charset';
}

if (constant('PHP_VERSION_ID') !== PHP_VERSION_ID) {
    $issues[] = 'constant PHP_VERSION_ID';
}

if (count($issues) > 0) {
    echo 'platform-fail:', implode(',', $issues), "\n";
    return false;
}

echo "platform-ok\n";
return true;
