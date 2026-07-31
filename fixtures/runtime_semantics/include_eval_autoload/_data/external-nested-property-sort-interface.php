<?php

namespace Fixture\External;

interface ExternalNestedHookManager
{
    public function add($name, $priority, $value);

    public function dispatch($name, $parameters);
}
