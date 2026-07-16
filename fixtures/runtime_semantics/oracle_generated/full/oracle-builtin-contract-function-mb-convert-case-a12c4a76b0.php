<?php
// oracle-probe: id=oracle-builtin-contract-function-mb-convert-case-a12c4a76b0 area=builtin_contract kind=function symbol=mb_convert_case source=ext/mbstring/mbstring.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=0 php_ref_optional_reason=missing_reference_extension oracle_probe_id=oracle-builtin-contract-function-mb-convert-case-a12c4a76b0 failure_category=builtin_contract requires_ref_extension=mbstring
try {
    $result = \mb_convert_case();
    echo "return:\n";
    var_dump($result);
} catch (Throwable $error) {
    echo "throw:", get_class($error), ":", $error->getMessage(), "\n";
}
