<?php
add_filter("the_content", "wp_like_plugin_alpha");

function wp_like_plugin_alpha($html) {
    return str_replace("wordpress-like", "wordpress-like|alpha=1", $html);
}
