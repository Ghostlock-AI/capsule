# Contributing to Capsule

Capsule is a permanently open-source project for kernel-level tracing and
sandboxing. It is intended to exist as long-term public infrastructure.

Ghostlock initiated Capsule, but does not claim permanent ownership or control.
If you want to lead, maintain, fork, or replace us by doing better work, you are
welcome to do so.

---

## Why this project exists

Application-level observability and enforcement no longer hold up in a world of
autonomous software. User-mode hooks are easy to bypass, and many existing tools
cannot reliably explain what code actually did once it executed.

As software shifts toward agents that write code, spawn processes, and act with
minimal supervision, the human role moves upstream: from approving outputs to
understanding and constraining behavior.

Kernel-level tracing is the correct layer to anchor trust. It observes execution
below the application boundary, where behavior cannot opt out of being recorded.
Today, however, kernel tracing remains fragmented, hard to use, and limited to
specialists.

Capsule exists to make this layer practical.

---

## Project principles

- **Permanent openness**  
  Capsule will remain open source and publicly available. It will never be
  closed, dual-licensed into a proprietary core, or withdrawn from the
  community.

- **Control follows the work**  
  Authority in this project is earned through sustained, high-quality
  contribution—not by origin, title, or affiliation.

- **Correctness over velocity**  
  We prioritize correctness, clarity, and debuggability over short-term speed.

- **Explicit behavior**  
  We favor systems that can explain what happened, not just what was intended.

---

## Stewardship and maintainership

Ghostlock does not assume permanent stewardship of Capsule.

We explicitly welcome contributors who want to:
- drive architectural direction,
- act as long-term maintainers,
- or steward Capsule as durable public infrastructure.

If another individual or group becomes the primary maintainer through sustained
work, stewardship is expected to follow the work.

If Ghostlock ceases to maintain Capsule, others are encouraged to continue the
project without restriction.

---

## Getting started

Capsule is written primarily in **Rust** and targets **Linux**.

Current status:
- Platform: Linux (aarch64; others in progress)
- Architecture: early-stage and evolving

Build and run instructions are documented in the README. If they are incomplete
or incorrect, improving them is considered a valid contribution.

---

## What we are interested in

We welcome contributions in areas such as:

- Kernel tracing and syscall instrumentation
- Sandboxing and enforcement mechanisms
- Performance and overhead reduction
- Event modeling and trace semantics
- Tests, benchmarks, and verification
- Documentation that explains non-obvious behavior

If you are unsure whether something fits, open an issue and discuss it.

---

## Pull requests

- Keep changes focused and scoped
- Explain *why* a change is needed, not just *what* it does
- Expect review and discussion
- Large or architectural changes should be proposed via issues first

Review is collaborative, not adversarial.

---

## Code expectations

- Follow existing Rust formatting and conventions
- Prefer clarity over cleverness
- Avoid unnecessary abstraction
- Document security-sensitive behavior
- Be explicit about trade-offs

This project values maintainability and correctness over novelty.

---

## Decision-making

Technical decisions are made through:
- discussion in issues or discussions,
- review of proposed changes,
- and rough consensus among active maintainers.

When consensus cannot be reached, contributors closest to the affected code
paths make the final call.

---

## Forks and continuity

Forking is explicitly allowed and respected.

If a fork better serves the project’s goals or evolves faster, that is a valid
outcome of open-source development.

Capsule is intended to survive its original authors.

---

## Security issues

If you believe you have found a security vulnerability, please do not open a
public issue. Report it privately using the contact information in `SECURITY.md`
(if present) or by contacting the maintainers directly.

---

## License

Capsule is licensed under the **MIT License**.

By contributing to this repository, you agree that your contributions are
licensed under the MIT License, with no additional restrictions.

No Contributor License Agreement (CLA) is required.
