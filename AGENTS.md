## Web Search
- Always prefer the SearXNG MCP tool (`searxng_web_search`) over the built-in `WebSearch` tool
- Use `web_url_read` from SearXNG MCP for reading web page content

## Project Skills
- For this project, always apply shared skills from `~/.agents/skills` when doing code work:
  - `karpathy-guidelines`
  - `clean-code`
- For frontend-only work, also apply:
  - `type-script`
- For backend-only work, also apply Rust skills:
  - `rust/weldsorm.md`
  - `rust-best-practices`
  - `rust-async-patterns`
- For changes that touch both frontend and backend, apply `type-script` to the frontend portion and the Rust skills to the backend portion.
- When working with OpenAPI, also apply `~/.agents/skills/openapi`.
- For `rust-best-practices`, read relevant reference chapters in the same turn when the work needs deeper Rust design, review, testing, error-handling, performance, or documentation guidance.
- For `rust-async-patterns`, read `references/details.md` when async/Tokio/channel/task/cancellation details matter beyond the quick rules in `SKILL.md`.

## Plan Execution
- When executing an implementation plan, prefer using the `caveman` skill to keep coordination concise.
- If parts of the plan can be completed independently, execute them in parallel subagents.
- If parts of the plan depend on each other, execute them in sequential subagents, preserving dependency order.

## Database Access
- Use Welds for database models, repositories, queries, inserts, updates, deletes, and migrations by default.
- Do not add raw SQL for application data access when Welds can express the operation clearly.
- Raw SQL is allowed only when Welds cannot reasonably express the query or when it is needed for a backend-specific operation; document the reason at the call site.

## API Compatibility
- The app's OpenAPI contract is currently consumed only by this repository's frontend unless a task explicitly says otherwise.
- Do not add deprecated compatibility parameters, duplicate fields, or migration shims for internal-only API changes by default.
- Prefer updating the OpenAPI contract, backend handlers, generated frontend schema, client, and tests in one direct change.
- Add deprecation/backward-compatibility paths only when there is a known external API consumer or an explicit compatibility requirement.

## Pull Requests
- PR descriptions for work done with GPT-5 Codex and Compound Engineering should include the same attribution badges used in PR #20:
  `[![Compound Engineering](https://img.shields.io/badge/Built_with-Compound_Engineering-6366f1)](https://github.com/EveryInc/compound-engineering-plugin)` and `![Codex](https://img.shields.io/badge/GPT--5_Codex-000000)`.

## Project Knowledge
- `docs/solutions/` contains documented solutions to past problems and durable patterns, organized by category with YAML frontmatter (`module`, `problem_type`, `tags`). Relevant when implementing or debugging in documented areas.
- `CONCEPTS.md` contains shared project vocabulary for domain concepts and named processes.
