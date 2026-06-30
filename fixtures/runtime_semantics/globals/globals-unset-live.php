<?php
// runtime-semantics: category=globals expect=pass
$x = 1;
unset($GLOBALS["x"]);
echo isset($x) ? "set" : "unset", "\n";
$x = 2;
echo $GLOBALS["x"], "\n";
$GLOBALS["nested"] = ["a" => 1, "b" => 2];
unset($GLOBALS["nested"]["a"]);
echo isset($nested["a"]) ? "bad" : "unset", ":", $GLOBALS["nested"]["b"], "\n";
