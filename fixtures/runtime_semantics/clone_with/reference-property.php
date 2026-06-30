<?php
// runtime-semantics: category=clone_with expect=pass
class CloneReferenceProperty {
    public mixed $value;
}

$source = "old";
$original = new CloneReferenceProperty();
$original->value =& $source;
$copy = clone $original;
$copy->value = "new";
echo $source;
