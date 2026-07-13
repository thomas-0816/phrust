<?php

namespace Acme\Providers\Http\Traits;

trait WithTransporterTrait
{
    public function transporterName(): string
    {
        return "transporter-" . static::class;
    }
}
