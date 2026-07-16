<?php
// oracle-probe: id=oracle-builtin-contract-function-crc32-9806dbc936 area=builtin_contract kind=function symbol=crc32 source=ext/standard/basic_functions.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=1 oracle_probe_id=oracle-builtin-contract-function-crc32-9806dbc936 failure_category=builtin_contract
$name = "crc32";
echo function_exists($name) ? "available\n" : "missing\n";
