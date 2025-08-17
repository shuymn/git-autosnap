# Rust Coding Standards

## 0. Scope and Terminology

* **Target**: All Rust code for applications (`bin`) and libraries (`lib`).
* **Terminology**: **MUST** / **SHOULD** / **MAY**.

---

## 1. Quality Gates (MUST)

* `cargo fmt --all --check` must pass.
* `cargo clippy --all-targets --all-features -D warnings` must pass.
* `cargo test` (unit, integration, doctest) must be green.
* Published crates must declare `rust-version` in `Cargo.toml` and meet the MSRV.
* `cargo doc` must produce no warnings for the public API (remove undocumented public items, prevent broken links).

---

## 2. Code Style (MUST/SHOULD)

* **Naming (MUST)**: Types & traits = `PascalCase`; functions/variables = `snake_case`; constants/static = `SCREAMING_SNAKE_CASE`; modules = `snake_case`.
* **Imports (SHOULD)**: List explicitly; wildcard (`*`) imports are prohibited by default. Stabilize grouping and ordering.
* **Readability (SHOULD)**: Limit a function’s responsibility (roughly 30–50 lines). Use early returns to keep nesting shallow.

---

## 3. Ownership, Borrowing, API Boundaries (MUST/SHOULD)

* **Entry Points (MUST)**: Prefer borrowing for arguments (`&str` / `&[T]` / `AsRef<T>` / `Into<T>`).
* **Return Values (SHOULD)**: Return owned data, or use `Cow` when appropriate.
* **Unnecessary Copies (MUST)**: Prohibit gratuitous `clone` / `to_owned`. Allow only when justified by measurements.
* **Lifetimes (SHOULD)**: Omit explicit annotations when elision rules suffice; if needed, keep the scope minimal.

---

## 4. Error Handling and Panics (MUST/SHOULD)

* **Public API (MUST)**: Recoverable failures must be returned as `Result<T, E>`. `unwrap` / `expect` are prohibited.
* **Error Types (SHOULD)**: Libraries should use typed errors (`enum`, `thiserror`, etc.). Applications may aggregate (e.g., `anyhow`).
* **Context (SHOULD)**: Attach cause and input information with `?` and `map_err` / `with_context`.
* **Panics (MUST)**: Reserve for invariant violations or implementation bugs. Document the rationale for `debug_assert!` / `assert!` with comments.

---

## 5. Public API Design (MUST/SHOULD)

* **Minimal Exposure (MUST)**: Use `pub(crate)` / `pub(super)` to avoid exposing unnecessary items.
* **Compatibility (MUST)**: Follow SemVer for breaking changes and document alternatives and migration steps.
* **Trait Bounds (SHOULD)**: Keep them minimal and use `where` clauses for readability. Avoid bloated generics.

---

## 6. Concurrency & Asynchrony (MUST/SHOULD)

* **Blocking Prohibited (MUST)**: Do not perform synchronous I/O or heavy CPU work on an async runtime. Isolate such work with `spawn_blocking`, etc., when needed.
* **Control (MUST)**: Always set timeouts, cancellation, retry limits, and buffer limits for channels/queues.
* **`Send` / `Sync` (MUST)**: Do not assume them implicitly; minimize shared state. `unsafe impl` is prohibited unless absolutely necessary and accompanied by a proof.

---

## 7. Logging & Observability (SHOULD/MUST)

* **Structured Logging (SHOULD)**: Use `tracing` and emit event name, correlation ID, size, duration, etc., as fields.
* **Sensitive Data (MUST)**: Never log secret tokens, personal data, or other confidential information.

---

## 8. Testing (MUST/SHOULD)

* **Coverage (MUST)**: Provide unit tests for success, failure, and edge cases. Verify I/O and HTTP via integration tests.
* **Doctests (SHOULD)**: Include usage examples for public APIs in `///` comments and keep doctests passing.
* **Property/Fuzz (MAY)**: Consider `proptest` / `cargo-fuzz` for critical components such as parsers.
* **Independence (MUST)**: Tests must not depend on order or global state.

---

## 9. Documentation (MUST/SHOULD)

* **Public Items (MUST)**: Document purpose, parameters, return values, errors, panic conditions, and examples with `///`.
* **Crate / Module (SHOULD)**: Use `//!` to describe overall design, feature flags, assumptions, and invariants.
* **Conditional API (SHOULD)**: Mark feature‑dependent items with `#[doc(cfg(feature = "..."))]`.

---

## 10. Dependencies & Build (MUST/SHOULD)

* **Minimalism (MUST)**: Depend only on what is necessary. Record justification for heavy or `unsafe` dependencies.
* **Feature Flags (SHOULD)**: Make optional functionality opt‑in; defaults should favor safety.
* **Auditing (SHOULD)**: Regularly run `cargo audit` / `cargo deny` to check for vulnerabilities and license issues.
* **Unused (SHOULD)**: Remove unused dependencies with `cargo udeps`.

---

## 11. `unsafe` & FFI (MUST)

* Keep `unsafe` blocks as small as possible. Precede them with a `// SAFETY:` comment that explains the invariant being relied upon and how it is maintained.
* In FFI, fix layout with `repr(C)` etc., and document pointer lifetimes, ownership, and thread safety.
* `mem::transmute` is prohibited. If unavoidable, attach a proof of equivalence.

---

## 12. Performance (SHOULD)

* Obtain benchmarks or profiles beforehand; drive optimizations **by measurement**.
* In hot paths, prioritize allocation reduction (`with_capacity`, iterator fusion) and elimination of unnecessary copies.
* Document the rationale and trade‑offs for any complexity introduced by optimizations.

---

## 13. Security & Input Validation (MUST)

* Validate length, range, and format of external input; consider safe defaults and rejection of unknown fields during deserialization.
* Apply resource limits (time, memory, recursion depth) to regexes, compression, recursive processing, etc.
* Mask or zero‑out confidential data when storing, displaying, or logging.

---

## Appendix A: Minimal Commands to Run in CI

```bash
cargo fmt --all --check
cargo clippy --all-targets --all-features -D warnings
cargo test --all
cargo doc --no-deps
```
