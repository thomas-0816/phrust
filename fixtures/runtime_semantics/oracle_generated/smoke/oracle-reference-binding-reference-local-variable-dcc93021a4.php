<?php
// oracle-probe: id=oracle-reference-binding-reference-local-variable-dcc93021a4 area=reference_binding kind=reference symbol=local-variable source=seed expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=1 oracle_probe_id=oracle-reference-binding-reference-local-variable-dcc93021a4 failure_category=reference_binding
$a = 1; $b =& $a; $b = 4; echo $a, ":", $b, "\n";
