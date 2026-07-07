# ADR 0012: Composer Source Mode

## Status

Accepted for the standard-library layer.

## Context

Composer compatibility is a practical integration target, but `composer.phar`,
network installs, plugins, and scripts require broader PHAR, process, and
network support than Standard library requires.

## Decision

Composer source mode with local fixtures is the required Standard library gate. PHAR is
optional and must be decided separately. Online Packagist, plugins, and scripts
are excluded from required gates.

## Consequences

Composer work focuses first on local autoload, platform checks, source-mode
bootstrap, and prioritized missing-function reports.
