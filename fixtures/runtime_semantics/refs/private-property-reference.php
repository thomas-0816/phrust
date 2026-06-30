<?php
// runtime-semantics: expect=pass
class PrivatePropertyReferenceBox {
    private string $value = "old";

    public function bind(): void {
        $alias =& $this->value;
        $alias = "new";
    }

    public function value(): string {
        return $this->value;
    }
}

$box = new PrivatePropertyReferenceBox();
$box->bind();
echo $box->value();
