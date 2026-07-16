<?php
// runtime-semantics: category=include_eval_autoload expect=pass

include __DIR__ . '/_data/external-global-property-dim-child.php';

class ExternalGlobalQuery
{
    public array $query_vars = [];
}

$GLOBALS['external_query'] = new ExternalGlobalQuery();
var_dump(external_global_property_dim_is_empty());
$GLOBALS['external_query']->query_vars['rest_route'] = '/fixture';
var_dump(external_global_property_dim_is_empty());
