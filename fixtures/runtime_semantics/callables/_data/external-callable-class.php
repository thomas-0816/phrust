<?php
class ExternalCallableTarget {
    public function decorate(string $value): string {
        return "[" . $value . "]";
    }
}
