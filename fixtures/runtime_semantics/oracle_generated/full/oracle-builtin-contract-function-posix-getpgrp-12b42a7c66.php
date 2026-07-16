<?php
// oracle-probe: id=oracle-builtin-contract-function-posix-getpgrp-12b42a7c66 area=builtin_contract kind=function symbol=posix_getpgrp source=ext/posix/posix.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=0 php_ref_optional_reason=missing_reference_extension oracle_probe_id=oracle-builtin-contract-function-posix-getpgrp-12b42a7c66 failure_category=builtin_contract requires_ref_extension=posix
$name = "posix_getpgrp";
echo function_exists($name) ? "available\n" : "missing\n";
