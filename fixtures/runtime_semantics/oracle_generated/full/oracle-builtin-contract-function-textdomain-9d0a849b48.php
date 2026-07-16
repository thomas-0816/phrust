<?php
// oracle-probe: id=oracle-builtin-contract-function-textdomain-9d0a849b48 area=builtin_contract kind=function symbol=textdomain source=ext/gettext/gettext.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=0 php_ref_optional_reason=missing_reference_extension oracle_probe_id=oracle-builtin-contract-function-textdomain-9d0a849b48 failure_category=builtin_contract requires_ref_extension=gettext
try {
    $result = \textdomain(__phrust_probe_unknown: 1);
    echo "return:\n";
    var_dump($result);
} catch (Throwable $error) {
    echo "throw:", get_class($error), ":", $error->getMessage(), "\n";
}
