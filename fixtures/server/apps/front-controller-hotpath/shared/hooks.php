<?php
$app_filters = [];

function add_filter($tag, $callback) {
    global $app_filters;

    if (!isset($app_filters[$tag])) {
        $app_filters[$tag] = [];
    }

    $callbacks = $app_filters[$tag];
    $callbacks[] = $callback;
    $app_filters[$tag] = $callbacks;
}

function apply_filters($tag, $value) {
    global $app_filters;

    $callbacks = $app_filters[$tag] ?? [];
    foreach ($callbacks as $callback) {
        $value = $callback($value);
    }
    return $value;
}

function app_resolve_route($path) {
    $routes = [
        "/" => "home",
        "/posts/42" => "single",
        "/admin/" => "admin",
    ];
    return $routes[$path] ?? "archive";
}
