<?php
// oracle-probe: id=oracle-builtin-contract-function-iterator-apply-3d26b6b82b area=builtin_contract kind=function symbol=iterator_apply source=ext/spl/php_spl.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=0 php_ref_optional_reason=missing_reference_extension oracle_probe_id=oracle-builtin-contract-function-iterator-apply-3d26b6b82b failure_category=builtin_contract requires_ref_extension=spl
$name = "iterator_apply";
echo function_exists($name) ? "available\n" : "missing\n";
