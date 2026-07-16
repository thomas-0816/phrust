<?php

function late_external_by_ref_property_target(&$items): void
{
    $items[] = 'changed';
}
