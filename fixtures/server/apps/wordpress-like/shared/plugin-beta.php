<?php
add_filter("the_content", "wp_like_plugin_beta");

function wp_like_plugin_beta($html) {
    return str_replace("\n", "|beta=1\n", $html);
}
