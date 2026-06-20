<?php

class PromotedProperties {
    public function __construct(
        public readonly string $id,
        protected int $count = 0,
        private(set) ?string $name = null,
    ) {
        echo $id;
    }
}
