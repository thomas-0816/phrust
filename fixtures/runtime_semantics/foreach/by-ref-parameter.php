<?php
// runtime-semantics: category=foreach expect=pass

function update_by_reference_parameter(array &$items): void
{
    foreach ($items as $key => &$item) {
        $item = $key . ':' . $item;
    }
    unset($item);
}

$items = ['a', 'b'];
update_by_reference_parameter($items);
var_dump($items);
