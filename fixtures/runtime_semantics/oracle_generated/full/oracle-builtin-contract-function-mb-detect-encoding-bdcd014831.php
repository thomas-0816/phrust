<?php
// oracle-probe: id=oracle-builtin-contract-function-mb-detect-encoding-bdcd014831 area=builtin_contract kind=function symbol=mb_detect_encoding source=ext/mbstring/mbstring.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=0 php_ref_optional_reason=missing_reference_extension oracle_probe_id=oracle-builtin-contract-function-mb-detect-encoding-bdcd014831 failure_category=builtin_contract requires_ref_extension=mbstring
try {
    $result = \mb_detect_encoding();
    echo "return:\n";
    var_dump($result);
} catch (Throwable $error) {
    echo "throw:", get_class($error), ":", $error->getMessage(), "\n";
}
