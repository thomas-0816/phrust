<?php
// runtime-semantics: category=variables expect=pass
echo $missing, "x\n";
echo isset($missing) ? "set" : "unset", "\n";
echo empty($missing) ? "empty" : "not-empty", "\n";
echo $missing ?? "fallback", "\n";
$missing;
echo "after\n";
