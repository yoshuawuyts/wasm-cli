---
name: code-quality
description: Improve code quality through structured refactoring — reduce nesting, split large files, and simplify control flow. Use during code review, refactoring, or when writing new code.
---

# Code Quality

Keep functions short, nesting shallow, and files focused on a single
responsibility. The rules below catch the most common structural problems and
give clear remedies.

## When to Use

Use this skill when:
- Reviewing or refactoring existing code
- Writing new functions, structs, or modules
- A file is growing beyond ~400 lines or a function beyond ~60 lines
- Nesting depth reaches 4+ levels

## Rules

### 1. Extract loops inside conditionals

When a loop appears inside a conditional branch, extract the loop body into its
own function.

**Before:**
```rust
if needs_update {
    for item in items {
        // 20 lines of processing …
    }
}
```

**After:**
```rust
if needs_update {
    process_items(&items);
}

fn process_items(items: &[Item]) {
    for item in items {
        // 20 lines of processing …
    }
}
```

### 2. Limit indentation to 3 levels

If code reaches 4 or more levels of indentation (relative to the function
body), extract the inner logic into a helper function.

**Before:**
```rust
fn sync(packages: &[Pkg]) {
    for pkg in packages {              // level 1
        if pkg.needs_sync() {          // level 2
            for dep in &pkg.deps {     // level 3
                if dep.is_stale() {    // level 4 ← too deep
                    refresh(dep);
                }
            }
        }
    }
}
```

**After:**
```rust
fn sync(packages: &[Pkg]) {
    for pkg in packages {
        if pkg.needs_sync() {
            sync_deps(&pkg.deps);
        }
    }
}

fn sync_deps(deps: &[Dep]) {
    for dep in deps {
        if dep.is_stale() {
            refresh(dep);
        }
    }
}
```

### 3. Split long if/else blocks

If both branches of an `if`/`else` are individually 60+ lines, extract each
branch into its own function.

**Before:**
```rust
if is_component {
    // 80 lines handling component case …
} else {
    // 70 lines handling module case …
}
```

**After:**
```rust
if is_component {
    handle_component(…);
} else {
    handle_module(…);
}
```

### 4. Prefer `match` over exhaustive `if let..else`

When an `if let..else` exhaustively matches every variant of an enum, replace
it with a `match` block. The most common case is `if let Some(..) { .. } else { .. }`.

**Before:**
```rust
if let Some(value) = option {
    process(value);
} else {
    use_default();
}
```

**After:**
```rust
match option {
    Some(value) => process(value),
    None => use_default(),
}
```

### 5. One primary type per file

Each file should contain at most one primary `struct` or `enum` and its `impl`
blocks. Move additional types into their own files and re-export from the
parent module.

**Exemptions:** Small helper enums, structs, and functions that exist only to
support the primary type may stay in the same file. Test modules (`#[cfg(test)]`)
are also exempt.

## Review Workflow

1. **Pick a crate** — start with the crate that has the most violations.
2. **List files** — sort by line count (descending).
3. **For each file**, check every rule above. Apply the smallest refactor that
   fixes each violation.
4. **Run `cargo xtask test`** — all of fmt, clippy, tests, and SQL check must
   pass before moving on.
5. **Commit per-crate** — one commit per crate keeps reviews manageable.

## Exemptions

- Small helper enums, structs, and free functions that are tightly coupled to
  a primary type may remain in the same file.
- `#[cfg(test)] mod tests` blocks are exempt from the one-type-per-file rule.
- Generated code (e.g., migration files produced by `cargo xtask sql migrate`)
  is exempt from all rules.
