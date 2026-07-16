<?php
// oracle-probe: id=oracle-builtin-contract-function-session-id-eead26c11c area=builtin_contract kind=function symbol=session_id source=ext/session/session.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=0 php_ref_optional_reason=missing_reference_extension oracle_probe_id=oracle-builtin-contract-function-session-id-eead26c11c failure_category=builtin_contract requires_ref_extension=session
try {
    $result = \session_id(__phrust_probe_unknown: 1);
    echo "return:\n";
    var_dump($result);
} catch (Throwable $error) {
    echo "throw:", get_class($error), ":", $error->getMessage(), "\n";
}
