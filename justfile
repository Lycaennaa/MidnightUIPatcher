# Distill is a cli wrapper making it easier for humans and agents to use commands by showing only errors/warnings and other non noise. https://github.com/lycaennaa/distill

# Common development commands for the Rust desktop app.
worktree_root := justfile_directory()
worktree_build_dir := worktree_root + "/.build"
cargo_target_dir := worktree_build_dir + "/cargo"
distill_available := `command -v distill >/dev/null 2>&1 && printf true || printf false`

default: help

help:
    @just --list

# Format Rust sources.
fmt:
    @cd "{{ worktree_root }}" && export CARGO_TARGET_DIR="{{ cargo_target_dir }}" && if [ "{{ distill_available }}" = "true" ]; then distill --cwd "{{ worktree_root }}" cargo fmt --all; else cargo fmt --all; fi

# Check formatting without changing files.
fmt-check:
    @cd "{{ worktree_root }}" && export CARGO_TARGET_DIR="{{ cargo_target_dir }}" && if [ "{{ distill_available }}" = "true" ]; then distill --cwd "{{ worktree_root }}" cargo fmt --all -- --check; else cargo fmt --all -- --check; fi

# Check Rust sources.
check:
    @cd "{{ worktree_root }}" && export CARGO_TARGET_DIR="{{ cargo_target_dir }}" && if [ "{{ distill_available }}" = "true" ]; then distill --cwd "{{ worktree_root }}" cargo check; else cargo check --quiet; fi

test:
    @cd "{{ worktree_root }}" && export CARGO_TARGET_DIR="{{ cargo_target_dir }}" && if [ "{{ distill_available }}" = "true" ]; then distill --cwd "{{ worktree_root }}" cargo test; else cargo test --quiet; fi

build:
    @cd "{{ worktree_root }}" && export CARGO_TARGET_DIR="{{ cargo_target_dir }}" && if [ "{{ distill_available }}" = "true" ]; then distill --cwd "{{ worktree_root }}" cargo build; else cargo build --quiet; fi

# Launch the desktop app.
run:
    @cd "{{ worktree_root }}" && export CARGO_TARGET_DIR="{{ cargo_target_dir }}" && if [ "{{ distill_available }}" = "true" ]; then distill --cwd "{{ worktree_root }}" cargo run; else cargo run --quiet; fi

# Run the full local verification set.
verify: fmt-check test build

# Maintainer-only check; requires ignored preview fixtures.
verify-local-preview root=".":
    @cd "{{ worktree_root }}" && export CARGO_TARGET_DIR="{{ cargo_target_dir }}" && if [ "{{ distill_available }}" = "true" ]; then distill --cwd "{{ worktree_root }}" cargo run -- --verify-local-preview {{root}}; else cargo run --quiet -- --verify-local-preview {{root}}; fi
