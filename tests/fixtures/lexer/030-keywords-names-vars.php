<?php
namespace Acme\Demo;

use \Vendor\Package;

interface Contract {}
trait SharedBehavior {}
enum Status { case Ready; }

readonly class Example extends BaseExample implements Contract
{
    public function run($input): mixed
    {
        $name = namespace\Thing::class;
        $callable = fn ($x) => match ($x) {
            1 => yield from $input,
            default => __METHOD__,
        };

        return [
            __LINE__,
            __FILE__,
            __DIR__,
            __CLASS__,
            __TRAIT__,
            __FUNCTION__,
            __NAMESPACE__,
        ];
    }
}

$var = $$dynamic;
café();
