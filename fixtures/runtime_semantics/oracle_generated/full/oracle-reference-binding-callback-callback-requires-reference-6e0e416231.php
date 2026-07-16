<?php
// oracle-probe: id=oracle-reference-binding-callback-callback-requires-reference-6e0e416231 area=reference_binding kind=callback symbol=callback-requires-reference source=seed expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=1 oracle_probe_id=oracle-reference-binding-callback-callback-requires-reference-6e0e416231 failure_category=reference_binding
function oracle_probe_mutate(&$value) { $value = $value + 1; }
$value = 1;
call_user_func_array("oracle_probe_mutate", [&$value]);
echo $value, "\n";
