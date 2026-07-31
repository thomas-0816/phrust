<?php
// runtime-semantics: category=include_eval_autoload expect=pass php_ref_required=1

function external_nested_property_sort_autoload($class)
{
    if ($class === 'Fixture\\External\\ExternalNestedPropertySort') {
        require __DIR__ . '/_data/external-nested-property-sort-child.php';
    } elseif ($class === 'Fixture\\External\\ExternalNestedPropertySortBase') {
        require __DIR__ . '/_data/external-nested-property-sort-base.php';
    } elseif ($class === 'Fixture\\External\\ExternalNestedListener') {
        require __DIR__ . '/_data/external-nested-property-sort-listener.php';
    } elseif ($class === 'Fixture\\External\\ExternalNestedHookManager') {
        require __DIR__ . '/_data/external-nested-property-sort-interface.php';
    } elseif ($class === 'Fixture\\External\\ExternalNestedInputValidator') {
        require __DIR__ . '/_data/external-nested-property-sort-validator.php';
    }
}

spl_autoload_register('external_nested_property_sort_autoload');
$listener = new \Fixture\External\ExternalNestedListener();

function run_external_nested_property_sort($fixture, $listener)
{
    $options = array('hooks' => $fixture);
    $listener->register($options['hooks']);
    $url = 'value';
    $headers = array();
    $data = array();
    $type = 'GET';
    var_dump($options['hooks']->dispatch('event', array(&$url, &$headers, &$data, &$type, &$options)));
}

run_external_nested_property_sort(
    new \Fixture\External\ExternalNestedPropertySortBase(),
    $listener
);
run_external_nested_property_sort(
    new \Fixture\External\ExternalNestedPropertySortInheritedChild(),
    $listener
);
run_external_nested_property_sort(
    new \Fixture\External\ExternalNestedPropertySortEmptyChild(),
    $listener
);
run_external_nested_property_sort(
    new \Fixture\External\ExternalNestedPropertySortLocalChild('local'),
    $listener
);
run_external_nested_property_sort(
    new \Fixture\External\ExternalNestedPropertySort('fixture'),
    $listener
);
