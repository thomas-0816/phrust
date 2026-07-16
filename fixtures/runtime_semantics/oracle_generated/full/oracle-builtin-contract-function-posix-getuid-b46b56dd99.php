<?php
// oracle-probe: id=oracle-builtin-contract-function-posix-getuid-b46b56dd99 area=builtin_contract kind=function symbol=posix_getuid source=ext/posix/posix.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=0 php_ref_optional_reason=missing_reference_extension oracle_probe_id=oracle-builtin-contract-function-posix-getuid-b46b56dd99 failure_category=builtin_contract requires_ref_extension=posix
$name = "posix_getuid";
echo function_exists($name) ? "available\n" : "missing\n";
