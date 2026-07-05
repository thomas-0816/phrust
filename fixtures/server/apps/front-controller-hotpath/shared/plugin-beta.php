<?php
add_filter("the_content", "app_plugin_beta");

function app_plugin_beta($html) {
    return str_replace("\n", "|beta=1\n", $html);
}
