<?php

function external_global_property_dim_is_empty(): bool
{
    return empty($GLOBALS['external_query']->query_vars['rest_route']);
}
