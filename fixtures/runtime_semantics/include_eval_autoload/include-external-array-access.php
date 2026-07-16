<?php
// runtime-semantics: category=include_eval_autoload expect=pass

require __DIR__ . '/_data/external-array-access-child.php';

class ExternalArrayAccessHolder
{
    public $items;
}

function external_array_access_name(ExternalArrayAccessHolder $holder): string
{
    return $holder->items[0]->name;
}

$holder = new ExternalArrayAccessHolder();
$holder->items = external_array_access_list();
echo external_array_access_name($holder), "\n";
