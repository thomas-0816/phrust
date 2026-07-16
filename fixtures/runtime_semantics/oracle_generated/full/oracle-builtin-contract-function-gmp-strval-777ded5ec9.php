<?php
// oracle-probe: id=oracle-builtin-contract-function-gmp-strval-777ded5ec9 area=builtin_contract kind=function symbol=gmp_strval source=ext/gmp/gmp.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=0 php_ref_optional_reason=missing_reference_extension oracle_probe_id=oracle-builtin-contract-function-gmp-strval-777ded5ec9 failure_category=builtin_contract requires_ref_extension=gmp
$name = "gmp_strval";
echo function_exists($name) ? "available\n" : "missing\n";
