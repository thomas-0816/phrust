<?php
// oracle-probe: id=oracle-builtin-contract-function-inflate-init-18f3c28350 area=builtin_contract kind=function symbol=inflate_init source=ext/zlib/zlib.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=0 php_ref_optional_reason=missing_reference_extension oracle_probe_id=oracle-builtin-contract-function-inflate-init-18f3c28350 failure_category=builtin_contract requires_ref_extension=zlib
try {
    $result = \inflate_init();
    echo "return:\n";
    var_dump($result);
} catch (Throwable $error) {
    echo "throw:", get_class($error), ":", $error->getMessage(), "\n";
}
