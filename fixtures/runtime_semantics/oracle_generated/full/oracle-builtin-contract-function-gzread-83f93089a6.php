<?php
// oracle-probe: id=oracle-builtin-contract-function-gzread-83f93089a6 area=builtin_contract kind=function symbol=gzread source=ext/zlib/zlib.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=0 php_ref_optional_reason=missing_reference_extension oracle_probe_id=oracle-builtin-contract-function-gzread-83f93089a6 failure_category=builtin_contract requires_ref_extension=zlib
$name = "gzread";
echo function_exists($name) ? "available\n" : "missing\n";
