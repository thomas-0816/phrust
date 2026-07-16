<?php
// oracle-probe: id=oracle-builtin-contract-function-posix-eaccess-5996b8c33c area=builtin_contract kind=function symbol=posix_eaccess source=ext/posix/posix.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=0 php_ref_optional_reason=missing_reference_extension oracle_probe_id=oracle-builtin-contract-function-posix-eaccess-5996b8c33c failure_category=builtin_contract requires_ref_extension=posix
$name = "posix_eaccess";
echo function_exists($name) ? "available\n" : "missing\n";
