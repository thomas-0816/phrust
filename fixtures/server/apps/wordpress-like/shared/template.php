<?php
function wp_like_template($view) {
    ob_start();
    echo "wordpress-like|route=", $view->route;
    echo "|class=", class_exists("WpLikePost") ? "yes" : "no";
    echo "|function=", function_exists("apply_filters") ? "yes" : "no";
    echo "|cookie=", $_COOKIE["wp_like"] ?? "none";
    foreach ($view->posts as $post) {
        echo "|post=", $post->id, ":", $post->title, ":", $post->meta["views"];
    }
    echo "\n";
    return ob_get_clean();
}
