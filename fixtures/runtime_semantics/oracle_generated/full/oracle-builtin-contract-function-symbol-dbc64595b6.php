<?php
// oracle-probe: id=oracle-builtin-contract-function-symbol-dbc64595b6 area=builtin_contract kind=function symbol=_ source=ext/gettext/gettext.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=0 php_ref_optional_reason=missing_reference_extension oracle_probe_id=oracle-builtin-contract-function-symbol-dbc64595b6 failure_category=builtin_contract requires_ref_extension=gettext
try {
    $result = \_();
    echo "return:\n";
    var_dump($result);
} catch (Throwable $error) {
    echo "throw:", get_class($error), ":", $error->getMessage(), "\n";
}
