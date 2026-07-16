<?php
// runtime-semantics: requires_ref_extension=sockets php_ref_optional_reason=reference-build-lacks-sockets

var_dump(socket_strerror(0));
var_dump(socket_strerror(1));
