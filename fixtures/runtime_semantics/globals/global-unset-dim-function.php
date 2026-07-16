<?php
$items = ["keep" => 1, "remove" => 2];

function remove_global_item(string $key): void {
    global $items;
    if (isset($items[$key])) {
        unset($items[$key]);
    }
}

remove_global_item("remove");
echo isset($items["remove"]) ? "set\n" : "unset\n";
echo $items["keep"], "\n";
