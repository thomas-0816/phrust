<?php
// oracle-probe: id=oracle-builtin-contract-function-ctype-upper-5619e9d6a0 area=builtin_contract kind=function symbol=ctype_upper source=ext/ctype/ctype.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=0 php_ref_optional_reason=missing_reference_extension oracle_probe_id=oracle-builtin-contract-function-ctype-upper-5619e9d6a0 failure_category=builtin_contract requires_ref_extension=ctype
$name = "ctype_upper";
echo function_exists($name) ? "available\n" : "missing\n";
