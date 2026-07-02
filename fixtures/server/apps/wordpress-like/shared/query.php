<?php
function wp_like_query_posts($route, $preview) {
    $rows = [
        ["id" => 42, "title" => "Hello Hotpath", "views" => 7],
        ["id" => 43, "title" => "Cache Warm", "views" => 11],
        ["id" => 44, "title" => "Array Lookup", "views" => 13],
    ];
    $posts = [];
    foreach ($rows as $row) {
        $row["route"] = $route;
        $row["preview"] = $preview;
        $posts[] = new WpLikePost($row["id"], $row["title"], $row);
    }
    return $posts;
}
