<?php
// oracle-probe: id=oracle-builtin-contract-function-imagecolorallocate-22ad65a421 area=builtin_contract kind=function symbol=imagecolorallocate source=ext/gd/gd.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=0 php_ref_optional_reason=missing_reference_extension oracle_probe_id=oracle-builtin-contract-function-imagecolorallocate-22ad65a421 failure_category=builtin_contract requires_ref_extension=gd
$name = "imagecolorallocate";
echo function_exists($name) ? "available\n" : "missing\n";
