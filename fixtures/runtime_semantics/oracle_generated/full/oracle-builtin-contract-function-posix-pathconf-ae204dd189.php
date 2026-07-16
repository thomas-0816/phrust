<?php
// oracle-probe: id=oracle-builtin-contract-function-posix-pathconf-ae204dd189 area=builtin_contract kind=function symbol=posix_pathconf source=ext/posix/posix.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=0 php_ref_optional_reason=missing_reference_extension oracle_probe_id=oracle-builtin-contract-function-posix-pathconf-ae204dd189 failure_category=builtin_contract requires_ref_extension=posix
$name = "posix_pathconf";
echo function_exists($name) ? "available\n" : "missing\n";
