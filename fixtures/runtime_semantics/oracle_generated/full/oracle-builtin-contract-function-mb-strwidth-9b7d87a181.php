<?php
// oracle-probe: id=oracle-builtin-contract-function-mb-strwidth-9b7d87a181 area=builtin_contract kind=function symbol=mb_strwidth source=ext/mbstring/mbstring.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=0 php_ref_optional_reason=missing_reference_extension oracle_probe_id=oracle-builtin-contract-function-mb-strwidth-9b7d87a181 failure_category=builtin_contract requires_ref_extension=mbstring
try {
    $result = \mb_strwidth();
    echo "return:\n";
    var_dump($result);
} catch (Throwable $error) {
    echo "throw:", get_class($error), ":", $error->getMessage(), "\n";
}
