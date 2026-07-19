<?php
function read_unset_reference_dimension(&$items)
{
    unset($items);
    echo $items['value'];
}

$items = array();
read_unset_reference_dimension($items);
echo "done\n";
