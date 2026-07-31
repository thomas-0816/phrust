<?php

function native_transition_multisite(): bool
{
    return false;
}

#[AllowDynamicProperties]
class NativeTransitionPropertyState
{
    public $base_prefix;
    public $prefix;
    public $blogid = 0;

    public function tables(string $scope): array
    {
        return match ($scope) {
            'global' => ['users' => $this->prefix . 'users'],
            'blog' => ['posts' => $this->prefix . 'posts'],
            default => ['legacy' => $this->prefix . 'legacy'],
        };
    }

    public function setPrefix(string $prefix, bool $setTableNames = true): string
    {
        if (preg_match('|[^a-z0-9_]|i', $prefix)) {
            return 'invalid';
        }

        $oldPrefix = native_transition_multisite() ? '' : $prefix;

        if (isset($this->base_prefix)) {
            $oldPrefix = $this->base_prefix;
        }

        $this->base_prefix = $prefix;

        if ($setTableNames) {
            foreach ($this->tables('global') as $table => $prefixedTable) {
                $this->$table = $prefixedTable;
            }

            if (native_transition_multisite() && empty($this->blogid)) {
                return $oldPrefix;
            }

            $this->prefix = $this->base_prefix . 'site_';

            foreach ($this->tables('blog') as $table => $prefixedTable) {
                $this->$table = $prefixedTable;
            }

            foreach ($this->tables('old') as $table => $prefixedTable) {
                $this->$table = $prefixedTable;
            }
        }

        return $oldPrefix;
    }
}
