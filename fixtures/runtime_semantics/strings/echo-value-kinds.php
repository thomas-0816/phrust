<?php
// runtime-semantics: category=strings expect=pass php_ref_required=1
// Echo across value kinds: fast scalar appends and the conversion
// fallbacks (float, object __toString, reference deref) must produce
// identical output. (Echo of an array is excluded: the engine emits the
// conversion warning without file/line — pinned in vm unit tests.)
class Speaks {
    public function __toString() {
        return "spoken";
    }
}

$s = "text";
$i = 42;
$f = 1.25;
$t = true;
$b = false;
$n = null;
echo $s, "|", $i, "|", $f, "|", $t, "|", $b, "|", $n, "|\n";

$obj = new Speaks();
echo $obj, "\n";

$ref =& $s;
echo $ref, "\n";
