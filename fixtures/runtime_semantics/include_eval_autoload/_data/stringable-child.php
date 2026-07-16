<?php

namespace Fixture\Stringable;

final class ExternalStringable
{
    public function __toString()
    {
        return 'external-string';
    }
}
