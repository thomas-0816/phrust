<?php
// oracle-probe: id=oracle-builtin-contract-function-mb-chr-49f8a0bfe5 area=builtin_contract kind=function symbol=mb_chr source=ext/mbstring/mbstring.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=0 php_ref_optional_reason=missing_reference_extension oracle_probe_id=oracle-builtin-contract-function-mb-chr-49f8a0bfe5 failure_category=builtin_contract requires_ref_extension=mbstring
try {
    $result = \mb_chr();
    echo "return:\n";
    var_dump($result);
} catch (Throwable $error) {
    echo "throw:", get_class($error), ":", $error->getMessage(), "\n";
}
