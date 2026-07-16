<?php

namespace Fixture\Throwable;

final class ExternalException extends \Exception
{
    public $type;

    public function __construct($message, $type, $code = 0)
    {
        parent::__construct($message, $code);
        $this->type = $type;
    }

    public function state()
    {
        return [$this->message, $this->code, $this->type];
    }
}
