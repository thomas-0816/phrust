<?php
// oracle-probe: id=oracle-builtin-contract-function-posix-isatty-f9a2a6ef5d area=builtin_contract kind=function symbol=posix_isatty source=ext/posix/posix.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=0 php_ref_optional_reason=missing_reference_extension oracle_probe_id=oracle-builtin-contract-function-posix-isatty-f9a2a6ef5d failure_category=builtin_contract requires_ref_extension=posix
$name = "posix_isatty";
echo function_exists($name) ? "available\n" : "missing\n";
