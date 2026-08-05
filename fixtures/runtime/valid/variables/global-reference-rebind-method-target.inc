<?php

final class CrossUnitNativeGlobalBinder
{
    public function bind(&$source): void
    {
        $GLOBALS['cross_unit_method_global_alias'] =& $source;
    }
}
