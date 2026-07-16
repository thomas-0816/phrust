<?php
// oracle-probe: id=oracle-builtin-contract-function-mhash-keygen-s2k-047b1c18f2 area=builtin_contract kind=function symbol=mhash_keygen_s2k source=ext/hash/hash.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=0 php_ref_optional_reason=missing_reference_extension oracle_probe_id=oracle-builtin-contract-function-mhash-keygen-s2k-047b1c18f2 failure_category=builtin_contract requires_ref_extension=hash
try {
    $result = \mhash_keygen_s2k();
    echo "return:\n";
    var_dump($result);
} catch (Throwable $error) {
    echo "throw:", get_class($error), ":", $error->getMessage(), "\n";
}
