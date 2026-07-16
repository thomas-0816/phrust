<?php
// oracle-probe: id=oracle-builtin-contract-function-gmp-pow-9018e6ddb5 area=builtin_contract kind=function symbol=gmp_pow source=ext/gmp/gmp.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=0 php_ref_optional_reason=missing_reference_extension oracle_probe_id=oracle-builtin-contract-function-gmp-pow-9018e6ddb5 failure_category=builtin_contract requires_ref_extension=gmp
$name = "gmp_pow";
echo function_exists($name) ? "available\n" : "missing\n";
