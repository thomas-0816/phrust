<?php

final class NativeParameterOverwriteDestructor {
    private string $label;

    public function __construct(string $label) {
        $this->label = $label;
    }

    public function __destruct() {
        echo "destruct:", $this->label, "|";
    }
}

function native_parameter_overwrite($value): int {
    $value = new NativeParameterOverwriteDestructor("always");
    return 1;
}

function native_parameter_conditional_overwrite($value, bool $replace): int {
    if ($replace) {
        $value = new NativeParameterOverwriteDestructor("conditional");
    }
    return 2;
}

echo native_parameter_overwrite(null), "\n";
echo native_parameter_conditional_overwrite(null, false), "\n";
echo native_parameter_conditional_overwrite(null, true), "\n";

$caller_owner = new NativeParameterOverwriteDestructor("caller");
echo native_parameter_overwrite($caller_owner), "|caller-live\n";
unset($caller_owner);
echo "done\n";
