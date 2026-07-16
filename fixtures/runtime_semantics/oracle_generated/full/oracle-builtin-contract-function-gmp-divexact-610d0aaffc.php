<?php
// oracle-probe: id=oracle-builtin-contract-function-gmp-divexact-610d0aaffc area=builtin_contract kind=function symbol=gmp_divexact source=ext/gmp/gmp.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=0 php_ref_optional_reason=missing_reference_extension oracle_probe_id=oracle-builtin-contract-function-gmp-divexact-610d0aaffc failure_category=builtin_contract requires_ref_extension=gmp
$name = "gmp_divexact";
echo function_exists($name) ? "available\n" : "missing\n";
