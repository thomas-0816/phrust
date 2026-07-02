<?php
$wp_like_filters = [];

function add_filter($tag, $callback) {
    global $wp_like_filters;

    if (!isset($wp_like_filters[$tag])) {
        $wp_like_filters[$tag] = [];
    }

    $callbacks = $wp_like_filters[$tag];
    $callbacks[] = $callback;
    $wp_like_filters[$tag] = $callbacks;
}

function apply_filters($tag, $value) {
    global $wp_like_filters;

    $callbacks = $wp_like_filters[$tag] ?? [];
    foreach ($callbacks as $callback) {
        $value = $callback($value);
    }
    return $value;
}

function wp_like_resolve_route($path) {
    $routes = [
        "/" => "home",
        "/posts/42" => "single",
        "/wp-admin/" => "admin",
    ];
    return $routes[$path] ?? "archive";
}
