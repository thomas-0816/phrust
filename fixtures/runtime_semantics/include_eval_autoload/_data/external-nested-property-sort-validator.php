<?php

namespace Fixture\External;

class ExternalNestedInputValidator
{
    public static function isPriority($value)
    {
        return is_int($value);
    }
}
