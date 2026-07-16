<?php
// runtime-semantics: category=callables expect=pass php_ref_required=1 regression_category=callables reference_behavior=stdout:cba regression_case=dynamic-expression-first-class-callable

$reverse = ("str" . "rev")(...);
echo $reverse("abc"), "\n";
