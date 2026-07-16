<?php

class LateStaticCallbackTarget {
    private static function inner(string $value): string {
        return "inner:$value";
    }

    public static function render(string $value): string {
        return static::inner($value);
    }
}

function render_late_static_callback(string $value): string {
    return LateStaticCallbackTarget::render($value);
}

class CallbackMetadata {
    public $render_callback = 'render_late_static_callback';
}

class OuterRenderFrame {
    public CallbackMetadata $metadata;

    public function __construct() {
        $this->metadata = new CallbackMetadata();
    }

    public function render(): string {
        return call_user_func($this->metadata->render_callback, 'ok');
    }
}
