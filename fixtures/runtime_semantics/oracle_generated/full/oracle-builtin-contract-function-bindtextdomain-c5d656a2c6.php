<?php
// oracle-probe: id=oracle-builtin-contract-function-bindtextdomain-c5d656a2c6 area=builtin_contract kind=function symbol=bindtextdomain source=ext/gettext/gettext.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=0 php_ref_optional_reason=missing_reference_extension oracle_probe_id=oracle-builtin-contract-function-bindtextdomain-c5d656a2c6 failure_category=builtin_contract requires_ref_extension=gettext
try {
    $result = \bindtextdomain();
    echo "return:\n";
    var_dump($result);
} catch (Throwable $error) {
    echo "throw:", get_class($error), ":", $error->getMessage(), "\n";
}
