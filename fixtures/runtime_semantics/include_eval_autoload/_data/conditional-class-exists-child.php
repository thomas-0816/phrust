<?php
if (!class_exists('ConditionalIncludeFirst', false)) {
    class ConditionalIncludeFirst {}
}

if (!class_exists('ConditionalIncludeSecond', false)) {
    class ConditionalIncludeSecond {
        public function value() {
            return 'visible';
        }
    }
}
