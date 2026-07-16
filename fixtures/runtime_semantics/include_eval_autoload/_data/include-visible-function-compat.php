<?php
if (!function_exists("include_visible_function")) {
    function include_visible_function(): string {
        return "compat";
    }
}

echo function_exists("include_visible_function") ? "visible\n" : "missing\n";
echo include_visible_function(), "-from-include\n";
