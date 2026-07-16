<?php
// oracle-probe: id=oracle-builtin-contract-function-posix-getcwd-255f4d9c07 area=builtin_contract kind=function symbol=posix_getcwd source=ext/posix/posix.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=0 php_ref_optional_reason=missing_reference_extension oracle_probe_id=oracle-builtin-contract-function-posix-getcwd-255f4d9c07 failure_category=builtin_contract requires_ref_extension=posix
$name = "posix_getcwd";
echo function_exists($name) ? "available\n" : "missing\n";
