<?php
// oracle-probe: id=oracle-builtin-contract-function-mb-parse-str-56139a0cc1 area=builtin_contract kind=function symbol=mb_parse_str source=ext/mbstring/mbstring.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=0 php_ref_optional_reason=missing_reference_extension oracle_probe_id=oracle-builtin-contract-function-mb-parse-str-56139a0cc1 failure_category=builtin_contract requires_ref_extension=mbstring
try {
    $result = \mb_parse_str();
    echo "return:\n";
    var_dump($result);
} catch (Throwable $error) {
    echo "throw:", get_class($error), ":", $error->getMessage(), "\n";
}
