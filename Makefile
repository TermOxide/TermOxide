# TermOxide: a framework for building terminal user interfaces in Rust.
#
# Run `make` (or `make help`) to read the guide, then use the targets below
# to build, test and run the project. Every target is a thin wrapper around
# cargo, so you can always run the cargo command yourself.

.RECIPEPREFIX = >
.DEFAULT_GOAL := help
SHELL := /bin/sh

CARGO ?= cargo
DEMO_APP := demos/external_app/Cargo.toml
DEMO_MOUSE := demos/external_app_mouse/Cargo.toml

define ABOUT
TermOxide
=========

TermOxide is a framework for terminal user interfaces (TUI) written in Rust.
You describe your interface as a tree of nodes, keep its state in reactive
signals, and the framework redraws only what changed when a signal moves.

  * Reactive: state lives in signals; reading one inside a view subscribes
    that view, writing one schedules a redraw of what depends on it.
  * Declarative: views are built with a node builder (or the `rsx!` macro)
    instead of drawing cells by hand.
  * Styled: a typed style value (colours, borders, padding, flex layout)
    is resolved and laid out into terminal cells.
  * One loop: the framework owns the terminal, the input thread and the
    input, update, build, render cycle, so an app only writes its logic.

The terminal backend is ratatui on top of crossterm.
endef

define RUN
Getting started
===============

Requirements
  * Rust stable (the workspace uses edition 2024), installed with rustup.
  * Rust nightly, for formatting only: `rustup toolchain install nightly`.
  * Nix users can run `nix develop` instead; the flake provides everything.

Build and test
  make build        compile the whole workspace
  make test         run every test of the workspace
  make doc          build the API docs and open them in your browser

Run a demo
  make demo         a counter app driven by the keyboard
  make demo-mouse   the same app, also reacting to the mouse
  Press `q` or Ctrl+C to quit a demo.

Use TermOxide in your own app
  Depend on the `termoxide` crate, build your view with
  `termoxide_rendering::builder`, keep state in `termoxide_reactive::Signal`
  and start everything with `termoxide::run_with_app`. The demos under
  `demos/` are small, complete examples to copy from.
endef

define LAYOUT
How the project is organised
============================

Cargo.toml              the workspace: every crate below is a member
crates/
  termoxide             the entry point: `run_with_app` and the main loop
  termoxide_reactive    signals, effects and owners (the reactive core)
  termoxide_rendering   the view node tree, the render loop, buffer diffing
                        and event routing
  termoxide_layout      the style value, inheritance, layers and layout
  termoxide_event       keyboard, mouse and resize events from the terminal
  termoxide_macros      procedural macros, such as `rsx!`
  termoxide_e2e         end to end tests through a real pseudo terminal
  cargo-extract         CI helper: lists the crates touched since a commit
demos/                  standalone example apps, each with its own Cargo.toml
assets/                 images used by the docs
.githooks/              pre-commit and commit-msg hooks (see `make setup`)
.github/                CI workflows, issue and pull request templates
rustfmt.toml            formatting rules (some need nightly rustfmt)
flake.nix               Nix development shell

Data flows one way: an event from termoxide_event updates a signal in
termoxide_reactive, the views that read it are rebuilt by
termoxide_rendering, styled and placed by termoxide_layout, and only the
changed cells are written to the terminal.
endef

define CONTRIBUTING
How to contribute
=================

1. Set up once
     make setup     point git at the hooks in .githooks/

2. Pick or open an issue
     Use the templates in .github/ISSUE_TEMPLATE. Every change should be
     linked to an issue.

3. Branch from `dev/trunk`, the integration branch
     feat/<area>/<topic>   a new feature, for example feat/style/arena
     fix/<area>/<topic>    a bug fix
     docs/<topic>          documentation only
   `main` only receives tested work from `dev/trunk`.

4. Write the change
     * Keep it small: one pull request does one thing.
     * Put tests in `crates/<crate>/tests/*.rs` and test the public API.
     * Avoid `unwrap`, `expect` and `panic!` in library code: return errors.
     * Doc comments explain what the signature does not already say.

5. Check before you push
     make check     runs what CI runs: format, clippy and every test

6. Commit with the repo's message format (checked by the hook and by CI)
     <scope>: <imperative lowercase subject>
   For example `style: add cell geometry and the border box model`.
     * The scope is the crate or area, in lowercase.
     * The header is 72 characters at most and ends with a letter or digit.
     * The first word is imperative: `add`, not `added`, `adds` or `adding`.
     * An optional body explains why, wrapped near 76 columns.
     * A public API break gets a `BREAKING CHANGE:` footer.

7. Open a pull request
     Fill in .github/pull_request_template.md, link the issue with
     `Closes #N` or `Part of #N`, and keep the title equal to the commit
     header it will be squashed into. For work that builds on an open pull
     request, stack it: target that pull request's branch instead of
     `dev/trunk`.
endef

export ABOUT RUN LAYOUT CONTRIBUTING

.PHONY: help about run layout contributing setup build test fmt fmt-check \
        lint check doc demo demo-mouse coverage clean

## Guide

help: ## Print the whole guide and the list of targets
> @printf '%s\n\n%s\n\n%s\n\n%s\n\n' "$$ABOUT" "$$RUN" "$$LAYOUT" "$$CONTRIBUTING"
> @printf 'Targets\n=======\n\n'
> @grep -E '^[a-z-]+:.*## ' $(MAKEFILE_LIST) | sed -E 's/:.*## /\t/' | expand -t 16 | sed 's/^/  /'

about: ## What TermOxide is
> @printf '%s\n' "$$ABOUT"

run: ## How to build, test and run the project
> @printf '%s\n' "$$RUN"

layout: ## How the repository is organised
> @printf '%s\n' "$$LAYOUT"

contributing: ## How to contribute
> @printf '%s\n' "$$CONTRIBUTING"

## Development

setup: ## Install the git hooks
> @sh .githooks/setup-hooks.sh

build: ## Compile the workspace
> $(CARGO) build --workspace --all-features

test: ## Run every test of the workspace
> $(CARGO) test --workspace --all-features

fmt: ## Format the code (needs nightly rustfmt)
> $(CARGO) +nightly fmt --all

fmt-check: ## Fail if the code is not formatted
> $(CARGO) +nightly fmt --all -- --check

lint: ## Run clippy, warnings as errors
> $(CARGO) clippy --workspace --all-targets --all-features -- -D warnings

check: fmt-check lint test ## Run what CI runs

doc: ## Build the API docs and open them
> $(CARGO) doc --workspace --no-deps --all-features --open

coverage: ## Print test coverage (needs cargo-llvm-cov)
> $(CARGO) llvm-cov --workspace --all-features --summary-only

## Demos

demo: ## Run the keyboard demo app
> $(CARGO) run --manifest-path $(DEMO_APP)

demo-mouse: ## Run the mouse demo app
> $(CARGO) run --manifest-path $(DEMO_MOUSE)

clean: ## Remove build artifacts
> $(CARGO) clean
