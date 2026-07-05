<?php
add_filter("the_content", "app_plugin_alpha");

function app_plugin_alpha($html) {
    return str_replace("front-controller-hotpath", "front-controller-hotpath|alpha=1", $html);
}
