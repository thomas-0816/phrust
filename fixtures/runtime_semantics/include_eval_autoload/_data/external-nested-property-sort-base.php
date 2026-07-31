<?php

namespace Fixture\External;

class ExternalNestedPropertySortBase implements ExternalNestedHookManager
{
    protected $groups = array();

    public function add($name, $priority, $value)
    {
        if (is_string($name) === false) {
            throw new \InvalidArgumentException('name');
        }
        if (is_callable($value) === false) {
            throw new \InvalidArgumentException('value');
        }
        if (ExternalNestedInputValidator::isPriority($priority) === false) {
            throw new \InvalidArgumentException('priority');
        }

        if (!isset($this->groups[$name])) {
            $this->groups[$name] = array(
                $priority => array(),
            );
        } elseif (!isset($this->groups[$name][$priority])) {
            $this->groups[$name][$priority] = array();
        }

        $this->groups[$name][$priority][] = $value;
    }

    public function dispatch($name, $parameters)
    {
        if (is_string($name) === false) {
            throw new \InvalidArgumentException('name');
        }
        if (is_array($parameters) === false) {
            throw new \InvalidArgumentException('parameters');
        }

        if (empty($this->groups[$name])) {
            return false;
        }

        if (!empty($parameters)) {
            $parameters = array_values($parameters);
        }

        ksort($this->groups[$name]);
        foreach ($this->groups[$name] as $priority => $callbacks) {
            foreach ($callbacks as $callback) {
                $callback(...$parameters);
            }
        }
        return true;
    }
}

class ExternalNestedPropertySortLocalChild extends ExternalNestedPropertySortBase
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

class ExternalNestedPropertySortEmptyChild extends ExternalNestedPropertySortBase
{
    public function dispatch($name, $parameters)
    {
        return parent::dispatch($name, $parameters);
    }
}

class ExternalNestedPropertySortInheritedChild extends ExternalNestedPropertySortBase
{
}
