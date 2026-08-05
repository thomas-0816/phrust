<?php

function bind_cross_unit_native_global_reference(&$source): void
{
    $GLOBALS['cross_unit_native_global_alias'] =& $source;
}
