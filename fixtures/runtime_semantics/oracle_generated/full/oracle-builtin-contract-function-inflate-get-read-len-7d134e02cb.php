<?php
// oracle-probe: id=oracle-builtin-contract-function-inflate-get-read-len-7d134e02cb area=builtin_contract kind=function symbol=inflate_get_read_len source=ext/zlib/zlib.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=0 php_ref_optional_reason=missing_reference_extension oracle_probe_id=oracle-builtin-contract-function-inflate-get-read-len-7d134e02cb failure_category=builtin_contract requires_ref_extension=zlib
try {
    $result = \inflate_get_read_len();
    echo "return:\n";
    var_dump($result);
} catch (Throwable $error) {
    echo "throw:", get_class($error), ":", $error->getMessage(), "\n";
}
