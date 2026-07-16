<?php
// oracle-probe: id=oracle-builtin-contract-function-msg-send-3da0cbd347 area=builtin_contract kind=function symbol=msg_send source=ext/sysvmsg/sysvmsg.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=0 php_ref_optional_reason=missing_reference_extension oracle_probe_id=oracle-builtin-contract-function-msg-send-3da0cbd347 failure_category=builtin_contract requires_ref_extension=sysvmsg
$name = "msg_send";
echo function_exists($name) ? "available\n" : "missing\n";
