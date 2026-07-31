<?php

namespace Fixture\External;

class ExternalNestedPropertySort extends ExternalNestedPropertySortBase
{
    protected $name;
    protected $state = array();

    public function __construct($name)
    {
        $this->name = $name;
    }

    public function dispatch($name, $parameters)
    {
        $result = parent::dispatch($name, $parameters);
        $this->state[] = $this->name . ':' . $name;
        return $result;
    }
}

function make_external_nested_property_sort()
{
    return new ExternalNestedPropertySort();
}
