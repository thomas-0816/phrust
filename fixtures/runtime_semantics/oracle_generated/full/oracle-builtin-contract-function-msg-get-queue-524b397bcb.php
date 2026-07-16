<?php
// oracle-probe: id=oracle-builtin-contract-function-msg-get-queue-524b397bcb area=builtin_contract kind=function symbol=msg_get_queue source=ext/sysvmsg/sysvmsg.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=0 php_ref_optional_reason=missing_reference_extension oracle_probe_id=oracle-builtin-contract-function-msg-get-queue-524b397bcb failure_category=builtin_contract requires_ref_extension=sysvmsg
$name = "msg_get_queue";
echo function_exists($name) ? "available\n" : "missing\n";
