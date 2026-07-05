<?php
function app_template($view) {
    ob_start();
    echo "front-controller-hotpath|route=", $view->route;
    echo "|class=", class_exists("HotpathPost") ? "yes" : "no";
    echo "|function=", function_exists("apply_filters") ? "yes" : "no";
    echo "|cookie=", $_COOKIE["app_hotpath"] ?? "none";
    foreach ($view->posts as $post) {
        echo "|post=", $post->id, ":", $post->title, ":", $post->meta["views"];
    }
    echo "\n";
    return ob_get_clean();
}
