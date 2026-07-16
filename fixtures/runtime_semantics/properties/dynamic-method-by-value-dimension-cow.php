<?php
// runtime-semantics: expect=pass

class DynamicByValueDimensionReceiver
{
    public function consume($value)
    {
        return $value;
    }
}

$source = array('blockName' => 'source');
$receiver = new DynamicByValueDimensionReceiver();
$receiver->consume($source['blockName']);

$copy = $source;
$copy['blockName'] = 'copy';

echo $source['blockName'], "\n";
echo $copy['blockName'], "\n";
