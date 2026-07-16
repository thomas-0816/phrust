<?php
namespace Fixture\ExternalStaticClosure;

class ValidatorHost
{
    public static $validator = 'default';

    public function wrongTarget($first, $second): string
    {
        return $first . $second;
    }

    public static function validate($value)
    {
        if (is_callable(static::$validator) && !is_string(static::$validator)) {
            return call_user_func(static::$validator, $value);
        }

        return false;
    }
}
