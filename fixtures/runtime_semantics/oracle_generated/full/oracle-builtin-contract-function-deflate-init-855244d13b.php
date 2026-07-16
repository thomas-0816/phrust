<?php
// oracle-probe: id=oracle-builtin-contract-function-deflate-init-855244d13b area=builtin_contract kind=function symbol=deflate_init source=ext/zlib/zlib.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=0 php_ref_optional_reason=missing_reference_extension oracle_probe_id=oracle-builtin-contract-function-deflate-init-855244d13b failure_category=builtin_contract requires_ref_extension=zlib
try {
    $result = \deflate_init();
    echo "return:\n";
    var_dump($result);
} catch (Throwable $error) {
    echo "throw:", get_class($error), ":", $error->getMessage(), "\n";
}
