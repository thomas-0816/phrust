<?php
// runtime-semantics: category=globals expect=pass
$x = 1;
$old =& $x;
unset($GLOBALS["x"]);
echo isset($x) ? "set" : "unset", ":", $old, "\n";
$x = 2;
echo $GLOBALS["x"], ":", $old, "\n";
$GLOBALS["nested"] = ["a" => 1, "b" => 2];
unset($GLOBALS["nested"]["a"]);
echo isset($nested["a"]) ? "bad" : "unset", ":", $GLOBALS["nested"]["b"], "\n";

$functionGlobal = 41;
function detachFunctionGlobalBinding(): void
{
    global $functionGlobal;
    $old =& $functionGlobal;
    unset($functionGlobal);
    echo isset($functionGlobal) ? "set" : "unset", ":", $old, ":", $GLOBALS["functionGlobal"], "\n";
    $functionGlobal = 73;
    echo $functionGlobal, ":", $old, ":", $GLOBALS["functionGlobal"], "\n";
}
detachFunctionGlobalBinding();
echo $functionGlobal, "\n";

$reboundGlobal = 41;
function rebindFunctionGlobalLocally(): void
{
    global $reboundGlobal;
    $old =& $reboundGlobal;
    $replacement = 73;
    $reboundGlobal =& $replacement;
    $reboundGlobal = 99;
    echo $reboundGlobal, ":", $replacement, ":", $old, ":", $GLOBALS["reboundGlobal"], "\n";
}
rebindFunctionGlobalLocally();
echo $reboundGlobal, "\n";
