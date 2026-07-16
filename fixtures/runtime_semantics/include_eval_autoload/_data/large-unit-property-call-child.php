<?php
class LargeUnitPropertyCall
{
    protected $host;

    public function __construct($host)
    {
        $this->host = $host;
    }

    public function paddingMethod0(): void {}
    public function paddingMethod1(): void {}
    public function paddingMethod2(): void {}
    public function paddingMethod3(): void {}
    public function paddingMethod4(): void {}
    public function paddingMethod5(): void {}
    public function paddingMethod6(): void {}
    public function paddingMethod7(): void {}
    public function paddingMethod8(): void {}
    public function paddingMethod9(): void {}
    public function paddingMethod10(): void {}
    public function paddingMethod11(): void {}
    public function paddingMethod12(): void {}
    public function paddingMethod13(): void {}

    public function connect()
    {
        return $this->parseHost($this->host);
    }

    public function parseHost($host)
    {
        return $host;
    }

    public function callOptional()
    {
        return $this->optionalValue();
    }

    public function optionalValue($value = 'default-ok')
    {
        return $value;
    }
}
