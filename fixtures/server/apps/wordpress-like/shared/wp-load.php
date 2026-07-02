<?php
require_once __DIR__ . "/hooks.php";
require_once __DIR__ . "/classes.php";
require_once __DIR__ . "/query.php";
require_once __DIR__ . "/template.php";
include_once __DIR__ . "/plugin-alpha.php";
include_once __DIR__ . "/plugin-beta.php";
include_once __DIR__ . "/plugin-alpha.php";

function wp_like_render_request($path, $preview) {
    $route = wp_like_resolve_route($path);
    $posts = wp_like_query_posts($route, $preview);
    $view = new WpLikeView($route, $posts);
    $html = wp_like_template($view);
    return apply_filters("the_content", $html);
}
