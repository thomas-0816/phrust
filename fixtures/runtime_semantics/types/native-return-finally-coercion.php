<?php

function native_return_after_finally(): int {
    try {
        return "42";
    } finally {
        echo "value-finally|";
    }
}

function native_return_error_after_finally(): int {
    try {
        return "not numeric";
    } finally {
        echo "error-finally|";
    }
}

function native_return_overridden_by_finally(): int {
    try {
        return "not numeric";
    } finally {
        return 7;
    }
}

function &native_reference_return_after_finally(): int {
    static $value = "43";
    try {
        return $value;
    } finally {
        echo "reference-finally|";
    }
}

function &native_reference_return_error_after_finally(): int {
    static $value = "not numeric";
    try {
        return $value;
    } finally {
        echo "reference-error-finally|";
    }
}

final class NativeReturnDestructor {
    public function __destruct() {
        echo "destruct|";
    }
}

function native_return_error_releases_frame(): int {
    $value = new NativeReturnDestructor();
    return "not numeric";
}

function native_return_success_releases_frame(): int {
    $value = new NativeReturnDestructor();
    return 1;
}

function native_return_owned_expression_error(): int {
    return new NativeReturnDestructor();
}

function native_return_error_after_finally_releases_frame(): int {
    $value = new NativeReturnDestructor();
    try {
        return "not numeric";
    } finally {
        echo "destructor-finally|";
    }
}

echo gettype(native_return_after_finally()), ":", native_return_after_finally(), "\n";

try {
    native_return_error_after_finally();
} catch (TypeError $error) {
    echo get_class($error), "\n";
}

echo native_return_overridden_by_finally(), "\n";

$reference =& native_reference_return_after_finally();
echo gettype($reference), ":", $reference, "\n";

try {
    $invalid_reference =& native_reference_return_error_after_finally();
} catch (TypeError $error) {
    echo get_class($error), "\n";
}

try {
    native_return_error_releases_frame();
} catch (TypeError $error) {
    echo "catch\n";
}

echo native_return_success_releases_frame(), "\n";

try {
    native_return_owned_expression_error();
} catch (TypeError $error) {
    echo "catch\n";
}

try {
    native_return_error_after_finally_releases_frame();
} catch (TypeError $error) {
    echo "catch\n";
}
