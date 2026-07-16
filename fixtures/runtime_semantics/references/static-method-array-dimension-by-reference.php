<?php
// runtime-semantics: expect=pass

class StaticDimensionReference
{
    public static function update(&$context): void
    {
        $context['enabled'] = true;
    }

    public static function run(): void
    {
        $settings = array('nested' => array());
        static::update($settings['nested']);
        echo json_encode($settings), "\n";
    }
}

StaticDimensionReference::run();
