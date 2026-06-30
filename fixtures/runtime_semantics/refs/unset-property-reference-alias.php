<?php
// runtime-semantics: expect=pass
class PropertyReferenceUnsetBox {
    public string $value = "old";
}

$box = new PropertyReferenceUnsetBox();
$alias =& $box->value;
unset($alias);
$box->value = "new";
echo isset($alias) ? "bad" : "unset", "|", $box->value;
