<?php

function external_nested_closure_container(): array
{
    $prefix = 'external';
    return array(
        'callbacks' => array(
            static function (string $value) use ($prefix): string {
                return "{$prefix} {$value}";
            },
        ),
    );
}

class ExternalClosurePrivateScope
{
    public function reduce(array $values): string
    {
        return array_reduce(
            $values,
            function (string $carry, string $value): string {
                return $carry . $this->decorate($value);
            },
            ''
        );
    }

    private function decorate(string $value): string
    {
        return "[{$value}]";
    }
}
