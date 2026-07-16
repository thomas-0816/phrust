<?php
// oracle-probe: id=oracle-builtin-contract-function-ctype-lower-105cabc510 area=builtin_contract kind=function symbol=ctype_lower source=ext/ctype/ctype.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=0 php_ref_optional_reason=missing_reference_extension oracle_probe_id=oracle-builtin-contract-function-ctype-lower-105cabc510 failure_category=builtin_contract requires_ref_extension=ctype
$name = "ctype_lower";
echo function_exists($name) ? "available\n" : "missing\n";
