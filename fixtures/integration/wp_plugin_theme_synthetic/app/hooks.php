<?php
$synthetic_hooks = [];

function synthetic_add_action($hook, $callback)
{
    global $synthetic_hooks;
    if (!isset($synthetic_hooks[$hook])) {
        $synthetic_hooks[$hook] = [];
    }
    $callbacks = $synthetic_hooks[$hook];
    $callbacks[] = $callback;
    $synthetic_hooks[$hook] = $callbacks;
}

function synthetic_do_action($hook, $state)
{
    global $synthetic_hooks;
    foreach ($synthetic_hooks[$hook] ?? [] as $callback) {
        $state = call_user_func($callback, $state);
    }
    return $state;
}
