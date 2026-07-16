<?php
class ReceiverAfterPrivateCall
{
    public array $innerBlocks;
    public array $innerContent = array(null);

    public function __construct()
    {
        $this->innerBlocks = array(new stdClass());
    }

    private function prepare(): array
    {
        return array();
    }

    public function render(array $options = array()): void
    {
        global $receiverAfterPrivateCallPost;
        static $root = null;
        $enabled = call_user_func(static fn ($value) => $value, true);
        if ($enabled && null === $root && false) {
            $root = $this;
        }
        $options = array_merge(array('dynamic' => true), $options);
        $computed = $this->prepare();
        foreach ($this->innerContent as $chunk) {
            if (!is_string($chunk)) {
                $inner = $this->innerBlocks[0];
                echo get_class($inner), ':', count($computed), ':', (int) $options['dynamic'], "\n";
            }
        }
    }
}

(new ReceiverAfterPrivateCall())->render(array('dynamic' => false));
