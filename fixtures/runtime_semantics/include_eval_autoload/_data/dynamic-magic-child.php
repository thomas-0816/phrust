<?php

namespace Fixture\Magic;

final class DynamicMethodMagicGetter
{
    protected $value = 'ok';

    public function __get($name)
    {
        $props = get_object_vars($this);
        if ($name === 'computed') {
            $method = 'get_value';
            return $this->$method();
        }
        if (array_key_exists($name, $props)) {
            return $this->$name;
        }
        return null;
    }

    protected function get_value()
    {
        return $this->value;
    }
}
