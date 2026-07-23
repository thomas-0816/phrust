<?php

class NativeDeclaredArrayBox
{
    private $value = array();

    public function replace($value)
    {
        $this->value = $value;
        return $this->value;
    }

    public function value()
    {
        return $this->value;
    }
}

$box = new NativeDeclaredArrayBox();
$box->replace(array('left' => 1, 'right' => 2));

echo implode(',', array_keys($box->value())), "\n";

class NativeDeclaredFloatBox
{
    public float $value = 0.0;

    public function replace($value)
    {
        return $this->value = $value;
    }
}

$floatBox = new NativeDeclaredFloatBox();
var_dump($floatBox->replace(7));
var_dump($floatBox->value);
