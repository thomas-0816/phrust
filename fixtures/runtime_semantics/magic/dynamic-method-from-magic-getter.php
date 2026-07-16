<?php

final class DynamicMethodMagicGetter
{
    protected $value = 'ok';

    public function __get($name)
    {
        $method = 'get_' . $name;
        return $this->$method();
    }

    protected function get_value()
    {
        return $this->value;
    }
}

$object = new DynamicMethodMagicGetter();
var_dump($object->value);
