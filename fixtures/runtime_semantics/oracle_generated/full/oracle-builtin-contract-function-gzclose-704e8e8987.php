<?php
// oracle-probe: id=oracle-builtin-contract-function-gzclose-704e8e8987 area=builtin_contract kind=function symbol=gzclose source=ext/zlib/zlib.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=0 php_ref_optional_reason=missing_reference_extension oracle_probe_id=oracle-builtin-contract-function-gzclose-704e8e8987 failure_category=builtin_contract requires_ref_extension=zlib
try {
    $result = \gzclose();
    echo "return:\n";
    var_dump($result);
} catch (Throwable $error) {
    echo "throw:", get_class($error), ":", $error->getMessage(), "\n";
}
