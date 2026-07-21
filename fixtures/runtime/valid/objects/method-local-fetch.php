<?php

class MethodLocalFetchFixture
{
    public function scalars(int $input): void
    {
        $null = null;
        $false = false;
        $true = true;
        $zero = 0;
        $negative = -7;
        $copy = $input;

        echo (int) ($null === null), '|';
        echo (int) $false, '|';
        echo (int) $true, '|';
        echo $zero, '|', $negative, '|', $copy, '|';
        echo $missing ?? 'quiet', "\n";
    }

    public function references(int &$value): void
    {
        $alias =& $value;
        echo $alias, '|';
        ++$value;
        echo $alias, "\n";
    }
}

$fixture = new MethodLocalFetchFixture();
$fixture->scalars(9);
$value = 4;
$fixture->references($value);
echo $value, "\n";
