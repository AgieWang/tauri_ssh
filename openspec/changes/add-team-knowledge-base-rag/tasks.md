## 1. Technical Spikes and Delivery Baseline

- [x] 1.1 Benchmark candidate local Embedding models with Chinese requirements, code identifiers, mixed-language text, and cross-project samples.
- [ ] 1.2 Verify local Embedding runtime compilation, model download, offline import, checksum validation, and inference on Windows, macOS, and Linux.
- [x] 1.3 Verify FTS5 availability and compare `trigram` and `unicode61` behavior for Chinese, version numbers, paths, classes, fields, and API routes.
- [x] 1.4 Benchmark local SQLite vector scanning at 10万 and 20万 chunks, recording query P50/P95, memory, database size, and rebuild time.
- [ ] 1.5 Probe the target Zentao test instance for version, authentication, pagination, rate limits, and requirement/task/Bug/test response fields.
- [ ] 1.6 Evaluate P0 source analyzers for Rust, TypeScript/JavaScript, Vue, Java, and SQL, including accuracy, package size, and cross-platform builds.
- [x] 1.7 Record selected dependencies, model, parser strategy, compatibility matrix, and performance thresholds in an implementation ADR.

## 2. Core Knowledge Database and Models

- [x] 2.1 Add incremental SQLite migrations for knowledge projects, releases, sources, documents, document versions, chunks, relations, jobs, and generation runs.
- [x] 2.2 Add migrations for Embedding profiles and chunk vectors, including the single-active-Profile constraint and required indexes.
- [x] 2.3 Add migrations for Zentao connections, mappings, cursors, entities, and entity relations without storing credential plaintext.
- [x] 2.4 Add migrations for code snapshots, files, symbols, and code relations with snapshot-scoped uniqueness.
- [x] 2.5 Define aligned Rust and TypeScript models for all new tables, command inputs, paginated outputs, job progress, citations, and error details.
- [x] 2.6 Implement Database-layer CRUD, soft deletion, transactions, pagination, and query indexes for core knowledge records.
- [x] 2.7 Implement FTS5 capability probing, table creation, content synchronization, and rebuild support.
- [x] 2.8 Implement vector BLOB encode/decode, dimension checks, norm persistence, and Profile-scoped read/write operations.
- [x] 2.9 Add in-memory migration and DAO tests covering uniqueness, soft deletion, FTS synchronization, vector validation, and one-active-Profile behavior.

## 3. Knowledge Catalog and Source Synchronization

- [x] 3.1 Create knowledge Command, Service, Database, and model modules following Command → Service → Database layering.
- [x] 3.2 Implement project and release CRUD, aliases, Git Tag/branch/Commit discovery, and explicit `unversioned` handling.
- [x] 3.3 Implement source CRUD for Git workspaces, local directories, single files, manual Markdown, and existing AI experiences.
- [x] 3.4 Implement canonical root validation, include/exclude rules, source-level remote permissions, and preview of the effective read scope.
- [x] 3.5 Implement Git document synchronization using read-only Commit/tree/Diff access without checkout, stash, reset, or branch switching.
- [x] 3.6 Implement local directory and single-file synchronization with canonical path checks, content hashes, rename/deletion handling, and incremental updates.
- [x] 3.7 Implement stable logical document identity and immutable document-version creation with source, project, release, path, Commit, and content hash.
- [x] 3.8 Implement persistent knowledge jobs, heartbeat, safe cancellation, interrupted-task recovery, idempotent checkpoints, polling, and Tauri progress events.
- [x] 3.9 Add catalog and source synchronization tests for unchanged content, failures, restart recovery, version ambiguity, and historical retention.

## 4. Parsing, Chunking, and Full-Text Search

- [x] 4.1 Define parser and chunker interfaces with deterministic normalization and versioned strategy identifiers.
- [x] 4.2 Implement Markdown parsing that preserves front matter, heading hierarchy, tables, code blocks, and source locations.
- [x] 4.3 Implement TXT/LOG, SQL, JSON, and YAML parsing with format-aware boundaries and explicit parse failures.
- [x] 4.4 Implement heading/structure-aware chunks with target/max size, overlap, content hash, token estimate, and full citation metadata.
- [x] 4.5 Implement incremental chunk replacement and FTS insert/update/delete synchronization inside safe transactions.
- [x] 4.6 Add document list, detail, version history, comparison, chunk preview, and citation-detail Commands and browser Dev API equivalents.
- [x] 4.7 Build a baseline retrieval fixture set covering requirement IDs, Chinese terms, paths, symbols, APIs, fields, SQL, versions, and conflicting documents.
- [x] 4.8 Add parser, chunker, FTS, version-history, and browser API integration tests.

## 5. Local Embedding and Vector Index

- [x] 5.1 Add the selected local Embedding runtime and model-cache abstraction without bundling an uncontrolled model download into release builds.
- [x] 5.2 Implement model download, internal mirror configuration, offline import, checksum verification, progress, retry, and cache cleanup.
- [x] 5.3 Implement the local Embedding provider for document/query prefixes, batch sizing, normalization, cancellation, and structured errors.
- [x] 5.4 Implement Profile fingerprint calculation using mode, model, revision, dimension, normalization, prefixes, chunk strategy, and normalization version.
- [x] 5.5 Implement Profile testing with real short-text inference and actual dimension persistence.
- [x] 5.6 Implement batch vector generation that skips matching content hash/Profile pairs and persists resumable progress.
- [x] 5.7 Implement metadata-filtered local cosine search against the active Profile.
- [x] 5.8 Add local Embedding tests for model cache, dimension mismatches, cancellation, interrupted recovery, incompatible Profiles, and deterministic query behavior.

## 6. Remote Embedding and Blue-Green Lifecycle

- [x] 6.1 Extend AI Provider capability metadata with an independent Embedding model without changing the chat default model.
- [x] 6.2 Implement OpenAI-compatible and Ollama-compatible Embedding adapters with capability-aware request fields and response validation.
- [x] 6.3 Implement timeout, conservative concurrency, batch limits, exponential backoff, rate-limit handling, and sanitized metrics.
- [x] 6.4 Implement the layered remote authorization check for system, source, document sensitivity, and content safety.
- [x] 6.5 Implement rebuild estimation for affected documents/chunks, local work, remote characters, expected disk use, and current-index availability.
- [x] 6.6 Implement independent blue-green index construction, completeness validation, atomic activation, old-Profile retention, rollback, and safe cleanup.
- [x] 6.7 Ensure local failures never trigger an unauthorized remote fallback and remote logs never contain source text or secrets.
- [x] 6.8 Add adapter contract tests, remote policy tests, unexpected-dimension tests, rebuild failure tests, activation transaction tests, and rollback tests.

## 7. Hybrid Retrieval and Evidence-Based RAG

- [x] 7.1 Implement deterministic query analysis for project aliases, releases, requirement IDs, Commits, code symbols, paths, APIs, tables, and fields.
- [x] 7.2 Implement project, release, code snapshot, document type, sensitivity, and permission hard filters before recall.
- [x] 7.3 Implement parallel FTS, active-Profile vector, and bounded relationship recall with per-channel diagnostics.
- [x] 7.4 Implement RRF or normalized weighted fusion with exact-project, exact-version, requirement-ID, confirmed-relation, verified-document, and stale penalties.
- [x] 7.5 Implement stable citations for documents, headings/pages, Zentao entities, Git Commits, code paths, symbols, and line ranges.
- [x] 7.6 Implement context preview and evidence-only RAG context assembly through the existing AI Provider.
- [x] 7.7 Implement answer rules for historical isolation, later-version separation, source conflicts, missing implementation/test evidence, and refusal to invent internal facts.
- [x] 7.8 Implement relation CRUD, manual confirmation, front-matter/Commit evidence import, and unconfirmed AI suggestions.
- [x] 7.9 Implement the fixed evaluation runner and metric persistence for Recall@K, MRR, citation accuracy, version leakage, refusal, and latency.
- [x] 7.10 Add retrieval and RAG regression tests for ambiguous projects, historical releases, semantic matches, conflicts, missing evidence, and Profile changes.

## 8. Zentao Knowledge Ingestion

- [x] 8.1 Implement sanitized Zentao connection CRUD using only secure credential references.
- [x] 8.2 Implement version/authentication/capability probing and an adapter registry that does not hard-code one API path as universal.
- [x] 8.3 Implement remote product/project/execution discovery and explicit mapping to knowledge projects and releases.
- [x] 8.4 Implement independent incremental cursors and paginated synchronization for stories and story changes.
- [x] 8.5 Implement incremental synchronization for tasks, worklogs, Bugs, test cases, test tasks/runs, builds, and releases according to probed capabilities.
- [x] 8.6 Implement normalized entity keys, content hashes, source timestamps, idempotent upserts, missing-count deletion confirmation, and resumable checkpoints.
- [x] 8.7 Build confirmed Zentao entity relations and parse configured Story/Task identifiers from Git Commit messages.
- [x] 8.8 Implement deterministic project overview, release requirements, traceability, task execution, test quality, change log, and open-risk Markdown generation.
- [x] 8.9 Implement optional AI summary generation separated from facts with Provider/model metadata and citation validation.
- [x] 8.10 Feed generated documents through the common document, chunk, FTS, active Profile, relation, citation, and RAG pipeline.
- [x] 8.10a Support explicitly authorized internal HTTP Zentao connections with per-connection persistence, UI risk confirmation, HTTPS-default validation, and sanitized audits while retaining same-origin read-only controls.
- [ ] 8.11 Add sanitized real-response fixtures and contract tests for authentication, missing capabilities, pagination, rate limits, HTML conversion, optional fields, and cursor recovery.
- [ ] 8.12 Complete one real read-only Zentao project/release acceptance sync and manually verify the generated traceability matrix.

## 9. Git and Local Source-Code Knowledge

- [x] 9.1 Implement code-source CRUD for existing Git workspaces and user-authorized non-Git directories, including language, scope, size, untracked, symlink, and remote-processing settings.
- [x] 9.2 Implement effective-scope preview with default exclusions, dependency/generated directories, binary detection, secret rules, and skip reasons.
- [x] 9.3 Implement immutable Git Commit/Tag/branch-head snapshots using object reads and Diff without changing the user's working tree.
- [x] 9.4 Implement isolated dirty-worktree snapshots with baseline Commit, branch, staged/modified/untracked sets, capture time, and content hashes.
- [x] 9.5 Implement non-Git local-directory snapshots with canonical boundaries and explicit non-historical semantics.
- [x] 9.6 Define the language analyzer interface and implement selected P0 analyzers for Rust, TypeScript/JavaScript/TSX, Vue, Java, and SQL.
- [x] 9.7 Implement `ast`, `structured_fallback`, `text_only`, and `skipped` quality levels with precise parser errors and partial-snapshot status.
- [x] 9.8 Extract stable symbols, signatures, qualified names, locations, documentation, routes, Commands, models, tables, columns, config keys, and tests.
- [x] 9.9 Resolve contains/imports/calls/implements/extends, Tauri IPC/events, HTTP API, Feign/Service/Mapper, SQL table, config, and tested-by relationships with evidence and confidence.
- [x] 9.10 Implement Git Diff/content-hash incremental analysis for added, modified, deleted, and renamed files plus dependent-relation invalidation.
- [x] 9.11 Implement symbol-boundary code chunks with project, snapshot, language, path, symbol, signature, line range, and sensitivity metadata.
- [x] 9.12 Generate deterministic repository, module, API/IPC, database, call-chain, config, test-map, Commit-change, release-implementation, and impact documents.
- [x] 9.13 Implement code file/symbol search, bounded call graph, snapshot comparison, impact analysis, and exact code citations.
- [x] 9.14 Link confirmed code evidence to Git Commits, Zentao requirements/tasks/tests, releases, and common knowledge relations.
- [x] 9.15 Add P0-language fixture tests, path/symlink security tests, dirty-worktree isolation tests, rename/incremental tests, relation-confidence tests, and code citation tests.

## 10. React UI and Desktop Integration

- [x] 10.1 Add knowledge routes, sidebar navigation, page shell, Zustand state, TypeScript API wrappers, and shared error handling.
- [x] 10.2 Implement project/release management, source management, scope preview, document list/detail/history, and sync-job views with Ant Design.
- [x] 10.2a Localize source-type options and support searchable multi-selection of registered Git workspaces, persisted as separate Git knowledge sources.
- [x] 10.3 Implement the knowledge ask/search page with project/version filters, recall-channel indicators, context preview, citations, conflicts, and evidence gaps.
- [x] 10.4 Implement Embedding settings for local/remote mode, model test, Profile fingerprint changes, rebuild estimate, confirmation, progress, activation, and rollback.
- [x] 10.5 Implement Zentao connection, capability matrix, remote project tree, project/release mapping, sync center, generated-document preview, and diff views.
- [x] 10.6 Implement code-source configuration, snapshot list, repository tree, symbol search, read-only code/line viewer, local relation graph, version comparison, and impact view.
- [x] 10.6a Support searching and multi-selecting registered Git workspaces when configuring a knowledge project, with legacy single-workspace migration compatibility.
- [x] 10.7 Ensure all browser Dev API routes call the same Services and policy checks as Tauri Commands.
- [x] 10.8 Add React component and API tests for forms, mode-dependent fields, authorization warnings, job progress, retries, citation opening, and sanitized errors.
- [x] 10.9 Validate every critical frontend flow with Codex in-app Browser or controlled Chrome, including console and network behavior.
- [x] 10.10 Validate the same critical flows in the packaged/development Tauri desktop runtime.

## 11. Security, Audit, Experience, and MCP Integration

- [x] 11.1 Implement a shared knowledge policy service for local path authorization, source permissions, sensitivity, remote Embedding, remote AI, MCP, and output filtering.
- [x] 11.2 Implement secret detection for private keys, certificates, common cloud keys, Tokens, passwords, connection strings, `.env`, credential files, and plaintext Git remote credentials.
- [x] 11.3 Ensure restricted content stores only allowed metadata/hash/skip reason and cannot reach remote providers, MCP, frontend content APIs, or normal logs.
- [x] 11.4 Add sanitized audit events for configuration, sync, Profile lifecycle, remote batches, Zentao access, code snapshots, document generation, relation confirmation, MCP, and RAG citations.
- [x] 11.5 Integrate existing `ai_experiences` as `experience` knowledge documents while preserving existing Commands and MCP behavior.
- [x] 11.6 Add the `knowledge` AI Skill scope with citation, historical isolation, evidence-gap, and no-invention rules.
- [x] 11.7 Expose read-only MCP tools for projects, releases, search, documents, citations, and ask operations.
- [x] 11.8 Add controlled MCP operations only for approved experience/relationship workflows, with no implicit Git or Zentao writes.
- [x] 11.9 Add security regression tests for path traversal, symlink escape, SSRF/redirect host changes, credential leakage, sensitive logs, remote policy bypass, and arbitrary MCP file reads.

## 12. Quality Gates, Rollout, and Acceptance

- [x] 12.1 Run Rust formatting, focused unit/integration tests, full `cargo test`, `cargo check`, and applicable `cargo clippy` checks.
- [x] 12.2 Run frontend formatting, type checking, component/API tests, and production build.
- [x] 12.3 Validate database migration from the current schema using a copied realistic database and verify rollback-by-disable without deleting legacy experience data.
- [ ] 12.4 Run the fixed retrieval evaluation before and after local/remote Profile changes and document acceptance thresholds.
- [ ] 12.5 Verify blue-green rebuild failure, interrupted recovery, atomic activation, rollback, and old-index cleanup on realistic data volume.
- [ ] 12.6 Verify one end-to-end release question across requirement, Zentao task, Git Commit, code symbol, SQL/API, test evidence, citations, conflicts, and evidence gaps.
- [ ] 12.7 Verify local-only operation without a vector server or remote model, including offline model import and knowledge search.
- [ ] 12.8 Verify remote Embedding and remote RAG use only explicitly authorized, non-sensitive sources and produce complete sanitized audits.
- [x] 12.9 Review all changed Rust, TypeScript, SQL, security-sensitive, and migration code with the required specialist reviewers and resolve findings.
- [x] 12.10 Update user/admin documentation for source setup, Zentao mapping, local model installation, remote authorization, code snapshots, recovery, and data cleanup.
- [x] 12.11 Roll out behind feature/settings gates in stages: catalog/FTS, local Embedding, hybrid RAG, Zentao, code analysis, then MCP.
- [ ] 12.12 Record final performance, compatibility, security, retrieval-quality, browser, desktop, and real-source acceptance evidence before enabling the feature by default.
