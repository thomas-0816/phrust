<?php
// oracle-probe: id=oracle-builtin-contract-function-posix-setsid-da194efec8 area=builtin_contract kind=function symbol=posix_setsid source=ext/posix/posix.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=0 php_ref_optional_reason=missing_reference_extension oracle_probe_id=oracle-builtin-contract-function-posix-setsid-da194efec8 failure_category=builtin_contract requires_ref_extension=posix
$name = "posix_setsid";
echo function_exists($name) ? "available\n" : "missing\n";
