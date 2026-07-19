<?php
function read_unset_reference_dimension(&$items)
{
    unset($items);
    echo $items['value'];
}

function read_uninitialized_plain_dimension()
{
    echo $items['value'];
}

$items = array();
read_unset_reference_dimension($items);
read_uninitialized_plain_dimension();
echo "done\n";
