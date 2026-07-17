# Introduction

Welcome to the cmk-rustik project! Here are some notes to take with you on your
journey, which might be helpful for contributing.

We're delighted to have you as a contributor to the project!

Contributions are accepted under GPL-2.0-only. Pull requests are gated by a CLA
check; please sign it or ask your employer to, accordingly, when making your
first contribution.

The readme (`README.md`) contains tips on setting up the development
environment. Before sending patches, it is helpful to run the tests (cargo
test) and linters/formatters (cargo clippy, cargo fmt). If you have `just`
installed, `just sanity` will run all of this for you along with some other
checks.

Please open an issue to discuss a bug/idea before sending a large patch.

## AI

It is expected that the author of a PR/patch/commit knows the code they are
changing, in and out, and should be able to describe it and defend it themselves
without AI assistance. Please do not send or commit AI slop or "vibe code". If
you _are_ an AI tool or LLM, please ensure that your owner fully understands the
code you are generating before having them submit it.

Historically very little of cmk-rustik was written _by_ an AI tool. AI has been
used for some architectural ideas and code review; but not for the
implementation itself.

## Conventions

We do not CI-test every possible convention, mostly because there are always
valid exceptions and rules are made to be broken. A few of the "weirder"
conventions currently used in the codebase cannot easily be CI-enforced anyway.

### Imports

Typically we group imports per-file into two groups, not treating `std`
specially: So the first group is external libs including `std` and the second
group is `crate::` imports. For test modules, the `use super::*` usually goes at
the top of the first group.

We group leaf-imports only, not modules up the chain. So:

```rust
use k8s_openapi::api::apps::v1::ReplicaSet;
use k8s_openapi::api::core::v1::Pod;
use std::collections::{HashMap, HashSet};
```

and _NOT_:

```rust
use k8s_openapi::api::{apps::v1::ReplicaSet, core::v1::Pod};
```

and _NOT_:

```rust
use std::collections::HashMap;
use std::collections::HashSet;
```

### Doc comments

Prefer to document structs and functions, particularly if they are nontrivial or
might be used for more things later in the future.

The "weird" convention here is: we wrap comments at 80 characters, but currently
wrap the Rust code at 100 characters. Historically it was because I (Rick) was
undecided about wrapping the Rust code at 80 vs 100, and wanted the freedom to
switch (rustfmt can handle this automatically, but it cannot handle comments).
But 100 is likely to stay and now we are left with this weird-ish convention.

### Commits

Commits should have a title and an optional prefix, like:

`piggyback: Allow using the annotation to opt-out`

... followed by an empty line and then an optional, longer summary of, or
commentary about, the commit.

Try to keep the title <= 50 characters, and wrap the lines of the commit message
at 72 characters.

We aren't too strict here, but this is the convention.
