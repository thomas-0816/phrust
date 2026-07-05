<?php
require_once __DIR__ . "/hooks.php";
require_once __DIR__ . "/classes.php";
require_once __DIR__ . "/query.php";
require_once __DIR__ . "/template.php";
include_once __DIR__ . "/plugin-alpha.php";
include_once __DIR__ . "/plugin-beta.php";
include_once __DIR__ . "/plugin-alpha.php";

function app_render_request($path, $preview) {
    $route = app_resolve_route($path);
    $posts = app_query_posts($route, $preview);
    $view = new HotpathView($route, $posts);
    $html = app_template($view);
    return apply_filters("the_content", $html);
}
