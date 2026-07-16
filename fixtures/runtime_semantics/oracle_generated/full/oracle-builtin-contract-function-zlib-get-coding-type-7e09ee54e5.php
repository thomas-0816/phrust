<?php
// oracle-probe: id=oracle-builtin-contract-function-zlib-get-coding-type-7e09ee54e5 area=builtin_contract kind=function symbol=zlib_get_coding_type source=ext/zlib/zlib.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=0 php_ref_optional_reason=missing_reference_extension oracle_probe_id=oracle-builtin-contract-function-zlib-get-coding-type-7e09ee54e5 failure_category=builtin_contract requires_ref_extension=zlib
try {
    $result = \zlib_get_coding_type(__phrust_probe_unknown: 1);
    echo "return:\n";
    var_dump($result);
} catch (Throwable $error) {
    echo "throw:", get_class($error), ":", $error->getMessage(), "\n";
}
