<?php
// runtime-semantics: category=refs expect=pass
class PropertyReferenceSource
{
    public $handle = 'initial';
}

$source = new PropertyReferenceSource();
$alias =& $source->handle;
$alias = 'through-alias';
echo $source->handle, "\n";
$source->handle = 'through-property';
echo $alias, "\n";
