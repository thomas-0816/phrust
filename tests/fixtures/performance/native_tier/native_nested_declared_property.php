<?php

class NativeNestedPropertyLeaf
{
    public $value;

    public function __construct()
    {
        $this->value = 41;
    }
}

class NativeNestedPropertyRoot
{
    public $leaf;

    public function __construct()
    {
        $this->leaf = new NativeNestedPropertyLeaf();
    }
}

$root = new NativeNestedPropertyRoot();
echo $root->leaf->value + 1, "\n";
