# Architecture

## Overview

```
Svelte frontend (src/)  ── invoke/listen ──▶  Tauri app (src-tauri/)  ──▶  yattat-core (crates/)
   views, store, dictionary, mock         commands, login webview,        Jira client (read-only), Tempo client,
                                          fetch jobs, automatic refresh   SQLite cache, fetch, statistics, export,
                                                                          secrets, i18n
```

All business logic lives in `yattat-core` and has no Tauri dependency. The crate is tested with
`cargo test` (unit tests, HTTP tests against `wiremock`). The Tauri layer is thin: it holds the state
(`AppState`), translates commands, starts fetches and reports progress and results as events to the
frontend. YaTTAT never writes to Jira.

## yattat-core

| Module | Purpose |
|---|---|
| `models` | Data model (JiraUser, Project, Issue, Worklog, Member, Team, Credential, …), serde camelCase |
| `settings` | `Settings` with defaults, teams and members, JSON file, `AppPaths`; `tidy()` cleans teams up and hands out colours |
| `i18n` | `Language` setting (System/De/En), `Lang` resolution from the OS locale, messages the backend sends (`t`, `tf`) |
| `error` | `Error` with English `Display` and `localized(lang)` for the user interface |
| `secrets` | `EncryptedSecretStore`: master key in the keyring, payload XChaCha20-Poly1305 in `secrets.enc` |
| `jira` | `JiraClient` (Cloud/DC): user search, projects, JQL search with pagination, worklogs per issue, issue by id; `jira::parse` (JSON → model, ADF comments) |
| `tempo` | `TempoCloud`: Tempo API v4 with token, worklogs per user (paginated), conversion to the cache model (`tempo-<id>`) |
| `storage` | SQLite (`rusqlite`, WAL): tables `issues`, `worklogs`, `projects`, `coverage`, `meta` |
| `fetch` | Fetch of a range for a member list; `compare_sources` reads both sources and compares per member and day. Jira source: JQL `worklogAuthor in (…) AND worklogDate …`, then worklogs per issue (4 in parallel), filter by author and day, count of foreign authors. Tempo source: worklogs per member from Tempo, issues by id from Jira. Then cache update including removal of vanished worklogs, and coverage |
| `stats` | Aggregation per team, member, day, project, issue, issue type and weekday; targets from the weekly pattern × weekly hours |
| `export` | CSV (semicolon, BOM for Excel, headers and date format by language) and JSON of the worklogs, optionally summarised per day/member/issue |

### Time model

- Worklogs carry `started` in Jira with the author's UTC offset. YaTTAT attributes every worklog to
  the calendar day in that time zone (`Worklog.date`). All day-based statistics use this day.
- The JQL search is widened by one day on each side because Jira evaluates `worklogDate` in the time
  zone of the searching user; the fine filtering happens locally.
- Target hours: `hours_per_weekday` (full-time pattern) × `weekly_hours / sum(pattern)` per member,
  only for days up to today.

### Cache and coverage

`coverage(author_id, date)` remembers which days were loaded for which author. A fetch for a range
only asks for the missing days (smallest contiguous gap per author, as one JQL for all affected
authors). "Refresh" forces the fetch of the displayed range; worklogs that no longer exist in the
source are then removed from the cache. The automatic refresh repeats the last requested range at the
configured interval, judged by the age of the loaded days.

## Tauri layer

| File | Purpose |
|---|---|
| `lib.rs` | App setup, plugins (single-instance, window-state, opener, dialog), logging, session restore |
| `state.rs` | `AppState`: store, settings, secrets, clients, user, `FetchStatus`, language resolution |
| `commands.rs` | All `#[tauri::command]`s: settings, login, project/user search, fetch, statistics, worklog lists, export, cache, comparison |
| `jobs.rs` | Install client (including cookie rotation → store), load the Tempo client from the token, restore session, fetch with progress events (source per settings), automatic refresh, source comparison |
| `login.rs` | Browser login window, cookie polling, logout including clearing the webview data (as in YaTTT) |

### Events (backend → frontend)

`auth-changed`, `settings-changed`, `fetch-status` (progress and result of a fetch), `data-changed`
(cache changed, views reload), `compare-progress` (source comparison), `login-window`, `login-progress`.

## Frontend

- Svelte 5 (runes), no router: `store.view` switches between team, member, issues and settings.
- `lib/i18n.svelte.ts`: dictionary (German and English), `t(key, params)` reads the language
  reactively; `lib/format.ts` formats dates, times and decimals by language.
- `lib/store.svelte.ts`: settings, auth, selected team, range (presets or custom), fetch status;
  `ensureData()` makes sure the cache covers the displayed range.
- `lib/api.ts`: typed wrappers around `invoke`; without Tauri (`npm run dev` in a browser)
  `lib/mock.ts` is used, an in-memory backend with two teams and deterministic sample worklogs.
- Charts are own SVG components (`BarChart`, `HBars`, `Donut`).
