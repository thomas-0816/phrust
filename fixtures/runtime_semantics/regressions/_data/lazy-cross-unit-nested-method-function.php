<?php

function lazy_cross_unit_nested_method_action($hook_name, ...$args) {
    global $lazy_cross_unit_hooks;
    return $lazy_cross_unit_hooks[$hook_name]->do_action($args);
}
