<?php

#[AllowDynamicProperties]
class NativePrivatePropertyConstructor
{
    private $blog_prefix;
    private $multisite;

    public function __construct()
    {
        $this->multisite = false;
        $this->blog_prefix = $this->multisite ? 'site:' : '';
    }

    public function prefix()
    {
        return $this->blog_prefix;
    }
}

$cache = new NativePrivatePropertyConstructor();
echo strlen($cache->prefix()), "\n";
