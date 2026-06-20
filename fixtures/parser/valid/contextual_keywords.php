<?php

namespace Vendor\match;

class ContextualKeywordMembers
{
    public const match = 1;
    public const readonly = 2;

    public function match(): void {}
    public function readonly(): void {}
    public function include(): void {}
}

$instance = new ContextualKeywordMembers();
$instance->match();
$instance->readonly();
$instance->include();
$instance->class;

ContextualKeywordMembers::match;
ContextualKeywordMembers::readonly;
ContextualKeywordMembers::match();

\Vendor\match\helper();
$runner = new \Vendor\match\ContextualKeywordMembers();
