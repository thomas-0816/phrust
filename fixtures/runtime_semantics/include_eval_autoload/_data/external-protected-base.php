<?php

class ExternalProtectedBase
{
    public const TOKEN = 'constant-ok';

    public function defaultPadding0(): void {}
    public function defaultPadding1(): void {}
    public function defaultPadding2(): void {}
    public function defaultPadding3(): void {}
    public function defaultPadding4(): void {}
    public function defaultPadding5(): void {}
    public function defaultPadding6(): void {}
    public function defaultPadding7(): void {}
    public function defaultPadding8(): void {}
    public function defaultPadding9(): void {}
    public function defaultPadding10(): void {}
    public function defaultPadding11(): void {}

    public function expose(): string
    {
        return $this->hidden();
    }

    protected function hidden(): string
    {
        return 'protected-ok';
    }

    public function exposeConstant(): string
    {
        return static::TOKEN;
    }

    public function callOptional(): string
    {
        return $this->optionalValue();
    }

    public function optionalValue($value = 'optional-ok'): string
    {
        return $value;
    }
}
