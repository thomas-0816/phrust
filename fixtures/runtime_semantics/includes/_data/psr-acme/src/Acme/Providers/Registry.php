<?php

namespace Acme\Providers;

use Acme\Providers\Http\Traits\WithTransporterTrait;

class Registry
{
    use WithTransporterTrait;

    public function label(): string
    {
        return $this->transporterName();
    }
}
