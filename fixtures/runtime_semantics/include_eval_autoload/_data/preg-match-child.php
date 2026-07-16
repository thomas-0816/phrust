<?php

namespace Fixture\Preg;

final class ExternalPregMatchFixture
{
    public static function parse($iri)
    {
        $hasMatch = preg_match(
            '/^((?P<scheme>[^:\/?#]+):)?(\/\/(?P<authority>[^\/?#]*))?(?P<path>[^?#]*)$/',
            $iri,
            $matches
        );

        return [$hasMatch, $matches];
    }
}
