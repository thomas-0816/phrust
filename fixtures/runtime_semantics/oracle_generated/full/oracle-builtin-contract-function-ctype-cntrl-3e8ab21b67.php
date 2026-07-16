<?php
// oracle-probe: id=oracle-builtin-contract-function-ctype-cntrl-3e8ab21b67 area=builtin_contract kind=function symbol=ctype_cntrl source=ext/ctype/ctype.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=0 php_ref_optional_reason=missing_reference_extension oracle_probe_id=oracle-builtin-contract-function-ctype-cntrl-3e8ab21b67 failure_category=builtin_contract requires_ref_extension=ctype
$name = "ctype_cntrl";
echo function_exists($name) ? "available\n" : "missing\n";
