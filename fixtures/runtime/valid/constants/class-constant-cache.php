<?php

class ConstantCacheBase
{
    public const LABEL = 'base';
    public const VALUES = ['kept'];

    public static function label(): string
    {
        return static::LABEL;
    }
}

class ConstantCacheChild extends ConstantCacheBase
{
    public const LABEL = 'child';
}

echo ConstantCacheBase::LABEL, '|', ConstantCacheBase::LABEL, "\n";
echo ConstantCacheBase::label(), '|', ConstantCacheChild::label(), '|', ConstantCacheBase::label(), "\n";

$copy = ConstantCacheBase::VALUES;
$copy[] = 'changed';
echo ConstantCacheBase::VALUES[0], '|', count(ConstantCacheBase::VALUES), '|', count($copy), "\n";
