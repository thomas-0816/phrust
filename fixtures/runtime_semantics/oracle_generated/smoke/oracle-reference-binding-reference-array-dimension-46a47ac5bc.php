<?php
// oracle-probe: id=oracle-reference-binding-reference-array-dimension-46a47ac5bc area=reference_binding kind=reference symbol=array-dimension source=seed expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=1 oracle_probe_id=oracle-reference-binding-reference-array-dimension-46a47ac5bc failure_category=reference_binding
$items = [1]; $alias =& $items[0]; $alias = 9; echo $items[0], "\n";
