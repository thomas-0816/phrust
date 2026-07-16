<?php
// oracle-probe: id=oracle-builtin-contract-function-imagecolorallocatealpha-0a4e3fb585 area=builtin_contract kind=function symbol=imagecolorallocatealpha source=ext/gd/gd.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=0 php_ref_optional_reason=missing_reference_extension oracle_probe_id=oracle-builtin-contract-function-imagecolorallocatealpha-0a4e3fb585 failure_category=builtin_contract requires_ref_extension=gd
$name = "imagecolorallocatealpha";
echo function_exists($name) ? "available\n" : "missing\n";
