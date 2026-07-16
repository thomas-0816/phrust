<?php
class ExternalMagicStaticParent
{
    public function __call($name, $arguments)
    {
        return get_class($this) . ':' . $name . ':' . count($arguments);
    }

    public static function __callStatic($name, $arguments)
    {
        return static::class . ':' . $name . ':' . count($arguments);
    }
}
