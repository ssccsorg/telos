# Notice

## Telos licensing

Telos is free software: you can redistribute it and/or modify it under
the terms of the GNU General Public License as published by the Free
Software Foundation, either version 3 of the License, or (at your option)
any later version.

The full license text is in the `LICENSE` file at the repository root.

## Apache-2.0 components

Some crates and components are licensed under the Apache License, Version
2.0 where their own `Cargo.toml` declares `license = "Apache-2.0"` (and
where the crate directory carries a `LICENSE-APACHE` file). Those crates
remain Apache-2.0 and are not relicensed by this notice. The full license
text is in `LICENSE-APACHE` at the repository root.

## Derived core

Telos absorbed a core derived from the Zed codebase. Upstream Zed is
licensed under GPL-3.0-or-later, with some components marked Apache-2.0;
that licensing is preserved for the absorbed core and its crates. See
`VENDORING.md` for the vendoring record and the absorb procedure.

## Third-party dependencies

Third-party dependencies are governed by their own licenses. They are
reported by the cargo-about tooling configured in `about.toml` at the
repository root; run `script/licenses-check.sh` to regenerate the
report.
