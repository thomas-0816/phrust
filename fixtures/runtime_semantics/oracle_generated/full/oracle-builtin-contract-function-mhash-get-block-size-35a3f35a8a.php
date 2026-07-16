<?php
// oracle-probe: id=oracle-builtin-contract-function-mhash-get-block-size-35a3f35a8a area=builtin_contract kind=function symbol=mhash_get_block_size source=ext/hash/hash.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=0 php_ref_optional_reason=missing_reference_extension oracle_probe_id=oracle-builtin-contract-function-mhash-get-block-size-35a3f35a8a failure_category=builtin_contract requires_ref_extension=hash
try {
    $result = \mhash_get_block_size();
    echo "return:\n";
    var_dump($result);
} catch (Throwable $error) {
    echo "throw:", get_class($error), ":", $error->getMessage(), "\n";
}
