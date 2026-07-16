<?php
// oracle-probe: id=oracle-builtin-contract-function-normalizer-is-normalized-1785e52a59 area=builtin_contract kind=function symbol=normalizer_is_normalized source=ext/intl/php_intl.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=0 php_ref_optional_reason=missing_reference_extension oracle_probe_id=oracle-builtin-contract-function-normalizer-is-normalized-1785e52a59 failure_category=builtin_contract requires_ref_extension=intl
try {
    $result = \normalizer_is_normalized();
    echo "return:\n";
    var_dump($result);
} catch (Throwable $error) {
    echo "throw:", get_class($error), ":", $error->getMessage(), "\n";
}
