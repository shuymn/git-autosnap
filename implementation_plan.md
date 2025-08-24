# Implementation Plan: Hybrid Git Operations for Performance

## Problem Statement
The `snapshot_once` operation using libgit2's `git_index_add_all()` is extremely slow (~30-70s) when traversing large ignored directories like `target/` (6.1GB). Research confirms that:
- libgit2's callback cannot prevent directory traversal (only filters what gets added to index)
- The performance issue is fundamental to libgit2's architecture, not related to bare vs non-bare repositories
- Git CLI handles this much more efficiently through multi-threading and better ignore handling

## Solution: Hybrid Approach
Use libgit2 for most operations but shell out to git CLI for performance-critical index operations.

## Implementation Details

### 1. Core Changes to `try_write_tree` (MUST)

**Location**: `src/gitlayer.rs::try_write_tree()`

**Current approach** (slow):
```rust
index.add_all(["*"], IndexAddOption::DEFAULT, None)?;
```

**New approach** (fast):
```rust
// Use git CLI for efficient index building that respects .gitignore
// without traversing ignored directories
Command::new("git")
    .args([
        "--git-dir", ".autosnap",
        "--work-tree", ".",
        "add", 
        "--all",
        "--ignore-errors"  // Continue even if some files can't be read
    ])
    .status()?;
```

### 2. Helper Function for Git CLI Operations (MUST)

Create a helper to safely execute git commands with proper error handling:

```rust
fn git_cli_add_all(git_dir: &Path, work_tree: &Path) -> Result<()> {
    let status = Command::new("git")
        .args([
            "--git-dir", git_dir.to_str().context("invalid git_dir path")?,
            "--work-tree", work_tree.to_str().context("invalid work_tree path")?,
            "add",
            "--all",
            "--ignore-errors"
        ])
        .stderr(Stdio::null())  // Suppress stderr for cleaner output
        .status()
        .context("failed to execute git add command")?;
    
    if !status.success() {
        bail!("git add --all failed with status: {}", status);
    }
    Ok(())
}
```

### 3. Modified `try_write_tree` Implementation (MUST)

```rust
fn try_write_tree(repo: &Repository, repo_root: &Path) -> std::result::Result<Oid, git2::Error> {
    let autosnap_dir = repo_root.join(".autosnap");
    
    // Use git CLI for efficient staging
    if let Err(e) = git_cli_add_all(&autosnap_dir, repo_root) {
        return Err(git2::Error::from_str(&e.to_string()));
    }
    
    // Read the updated index back
    let mut index = repo.index()?;
    index.read(true)?;  // Force re-read from disk
    
    // Remove .autosnap and .git if they got picked up
    let _ = index.remove_all([".autosnap", ".git"], None);
    
    index.write()?;
    index.write_tree()
}
```

### 4. Keep libgit2 for Other Operations (MUST)

Continue using libgit2 for operations where it performs well:
- Repository initialization
- Commit creation
- Tree/blob reading
- Diff operations (when not comparing working tree)
- History traversal

### 5. Update diff Function for Consistency (SHOULD)

The `build_working_tree_from_status` function in diff should also use the hybrid approach when building trees from the working directory, as it suffers from the same performance issue.

### 6. Testing Strategy (MUST)

1. **Performance tests**: Verify snapshot_once completes in <1s (not 30-70s)
2. **Correctness tests**: Ensure all non-ignored files are captured
3. **Edge cases**: 
   - Binary files
   - Symlinks  
   - Files with special characters
   - Very long paths

### 7. Documentation Updates (SHOULD)

Update CLAUDE.md to note:
- The hybrid approach and why it's necessary
- Performance characteristics
- Git CLI dependency for optimal performance

## Migration Path

### Phase 1: Implement and Test (Current)
1. Implement `git_cli_add_all` helper
2. Update `try_write_tree` to use git CLI
3. Test performance improvement
4. Verify correctness with existing tests

### Phase 2: Optimize Further (Future)
1. Consider using git CLI for status operations if needed
2. Profile other slow operations
3. Document performance characteristics

## Success Criteria

1. `snapshot_once` completes in <1 second (down from 30-70s)
2. All existing tests pass
3. No regression in snapshot completeness
4. Clear documentation of the hybrid approach

## Out of Scope

- Complete replacement of libgit2 (only targeted optimization)
- Custom ignore file parser
- Multi-threading implementation
- Caching mechanisms

## Notes

This hybrid approach is a pragmatic solution that:
- Solves the immediate performance problem
- Maintains compatibility with existing code
- Uses the right tool for each job
- Is maintainable and well-documented

The research clearly shows this is not a "hack" but rather a recognized pattern when dealing with libgit2's performance limitations in large repositories.
