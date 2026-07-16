<?php
// oracle-probe: id=oracle-builtin-contract-function-zlib-get-coding-type-80fd872d09 area=builtin_contract kind=function symbol=zlib_get_coding_type source=ext/zlib/zlib.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=0 php_ref_optional_reason=missing_reference_extension oracle_probe_id=oracle-builtin-contract-function-zlib-get-coding-type-80fd872d09 failure_category=builtin_contract requires_ref_extension=zlib
$name = "zlib_get_coding_type";
echo function_exists($name) ? "available\n" : "missing\n";
