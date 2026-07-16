<?php
// oracle-probe: id=oracle-builtin-contract-function-ctype-punct-78e50e8ce2 area=builtin_contract kind=function symbol=ctype_punct source=ext/ctype/ctype.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=0 php_ref_optional_reason=missing_reference_extension oracle_probe_id=oracle-builtin-contract-function-ctype-punct-78e50e8ce2 failure_category=builtin_contract requires_ref_extension=ctype
$name = "ctype_punct";
echo function_exists($name) ? "available\n" : "missing\n";
