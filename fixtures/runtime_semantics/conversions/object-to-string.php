<?php
// runtime-semantics: expect=pass
class StringableBox
{
    public function __toString()
    {
        return "box";
    }
}

$box = new StringableBox();
echo (string) $box, "\n";
