<?php
// oracle-probe: id=oracle-builtin-contract-function-session-write-close-04b6a3b2ab area=builtin_contract kind=function symbol=session_write_close source=ext/session/session.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=0 php_ref_optional_reason=missing_reference_extension oracle_probe_id=oracle-builtin-contract-function-session-write-close-04b6a3b2ab failure_category=builtin_contract requires_ref_extension=session
try {
    $result = \session_write_close(__phrust_probe_unknown: 1);
    echo "return:\n";
    var_dump($result);
} catch (Throwable $error) {
    echo "throw:", get_class($error), ":", $error->getMessage(), "\n";
}
