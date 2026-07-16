<?php
// runtime-semantics: expect=pass regression_category=closures reference_behavior=stdout:class-scope-matrix regression_case=native-closure-class-context

class ClosureScopeBase
{
    public function instanceStaticClass(): string
    {
        $callback = function (): string {
            return static::class;
        };
        return $callback();
    }

    public static function staticStaticClass(): string
    {
        $callback = function (): string {
            return static::class;
        };
        return $callback();
    }

    public function instanceSelfClass(): string
    {
        $callback = function (): string {
            return self::class;
        };
        return $callback();
    }

    public static function staticSelfClass(): string
    {
        $callback = function (): string {
            return self::class;
        };
        return $callback();
    }

    public static function name(): string
    {
        return 'base';
    }

    public static function lateBoundStaticCall(): string
    {
        $callback = static function (): string {
            return static::name();
        };
        return $callback();
    }
}

class ClosureScopeChild extends ClosureScopeBase
{
    public static function name(): string
    {
        return 'child';
    }
}

$child = new ClosureScopeChild();
echo $child->instanceStaticClass(), "\n";
echo $child->staticStaticClass(), "\n";
echo $child->instanceSelfClass(), "\n";
echo $child->staticSelfClass(), "\n";
echo $child->lateBoundStaticCall(), "\n";
echo ClosureScopeChild::lateBoundStaticCall(), "\n";
