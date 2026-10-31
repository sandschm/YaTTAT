# Architektur

## Überblick

```
Svelte-Frontend (src/)  ── invoke/listen ──▶  Tauri-App (src-tauri/)  ──▶  yattat-core (crates/)
   Views, Store, Mock                     Commands, Login-Webview,          Jira-Client (nur lesend), SQLite-Cache,
                                          Abruf-Jobs, Auto-Aktualisierung   Abruf-Logik, Statistik, Export, Secrets
```

Die gesamte Fachlogik liegt in `yattat-core` und hat keine Tauri-Abhängigkeit. Das Crate ist mit
`cargo test` testbar (Unit-Tests, HTTP-Tests gegen `wiremock`). Die Tauri-Schicht ist dünn: sie
hält den Zustand (`AppState`), übersetzt Commands, startet den Abruf und meldet Fortschritt und
Ergebnisse als Events an das Frontend. YaTTAT schreibt nie nach Jira.

## yattat-core

| Modul | Aufgabe |
|---|---|
| `models` | Datenmodell (JiraUser, Project, Issue, Worklog, Member, Team, Credential, …), serde camelCase |
| `settings` | `Settings` mit Defaults, Teams und Mitgliedern, JSON-Datei, `AppPaths`; `tidy()` räumt Teams auf und vergibt Farben |
| `secrets` | `EncryptedSecretStore`: Master-Key im Schlüsselbund, Nutzdaten XChaCha20-Poly1305 in `secrets.enc` |
| `jira` | `JiraClient` (Cloud/DC): Benutzer-Suche, Projekte, JQL-Suche mit Pagination, Worklogs je Ticket; `jira::parse` (JSON → Modell, ADF-Kommentare) |
| `storage` | SQLite (`rusqlite`, WAL): Tabellen `issues`, `worklogs`, `projects`, `coverage`, `meta` |
| `fetch` | Abruf eines Zeitraums für eine Mitgliederliste: JQL `worklogAuthor in (…) AND worklogDate …`, dann Worklogs je Ticket (4 parallel), Filter nach Autor und Tag, Cache-Update inkl. Löschen verschwundener Worklogs, Coverage |
| `stats` | Aggregation pro Team, Mitglied, Tag, Projekt, Ticket, Tickettyp und Wochentag; Soll aus Wochenmuster × Wochenstunden |
| `export` | CSV (Semikolon, BOM für Excel) und JSON der Worklogs, optional pro Tag/Mitglied/Ticket zusammengefasst |

### Zeitmodell

- Worklogs tragen in Jira `started` mit dem UTC-Offset des Autors. YaTTAT ordnet jedes Worklog dem
  Kalendertag in dieser Zeitzone zu (`Worklog.date`). Alle Tagesstatistiken nutzen diesen Tag.
- Die JQL-Suche wird um je einen Tag erweitert, weil Jira `worklogDate` in der Zeitzone des
  suchenden Benutzers auswertet; die Feinfilterung passiert lokal.
- Soll-Stunden: `hours_per_weekday` (Vollzeit-Muster) × `weekly_hours / Summe(Muster)` je Mitglied,
  nur für Tage bis heute.

### Cache und Coverage

`coverage(author_id, date)` merkt sich, welche Tage für welchen Autor geladen wurden. Ein Abruf für
einen Zeitraum fragt nur die fehlenden Tage ab (kleinste zusammenhängende Lücke je Autor, als ein
JQL für alle betroffenen Autoren). „Aktualisieren“ erzwingt den Abruf des angezeigten Zeitraums;
dabei werden Worklogs, die in Jira nicht mehr existieren, aus dem Cache entfernt. Die automatische
Aktualisierung wiederholt den zuletzt angeforderten Zeitraum im eingestellten Intervall.

## Tauri-Schicht

| Datei | Aufgabe |
|---|---|
| `lib.rs` | App-Aufbau, Plugins (single-instance, window-state, opener, dialog), Logging, Sitzung wiederherstellen |
| `state.rs` | `AppState`: Store, Settings, Secrets, Client, Benutzer, `FetchStatus` |
| `commands.rs` | Alle `#[tauri::command]`s: Settings, Login, Projekt-/Benutzersuche, Abruf, Statistik, Worklog-Listen, Export, Cache |
| `jobs.rs` | Client installieren (inkl. Cookie-Rotation → Speicher), Sitzung wiederherstellen, Abruf mit Fortschritts-Events, Auto-Aktualisierung |
| `login.rs` | Browser-Login-Fenster, Cookie-Polling, Logout inkl. Löschen der Webview-Daten (wie YaTTT) |

### Events (Backend → Frontend)

`auth-changed`, `settings-changed`, `fetch-status` (Fortschritt und Ergebnis des Abrufs),
`data-changed` (Cache hat sich geändert, Views laden neu), `login-window`, `login-progress`.

## Frontend

- Svelte 5 (Runes), kein Router: `store.view` schaltet zwischen Team, Mitglied, Tickets, Einstellungen.
- `lib/store.svelte.ts`: Einstellungen, Auth, gewähltes Team, Zeitraum (Presets oder frei), Abrufstatus;
  `ensureData()` sorgt dafür, dass der Cache den angezeigten Zeitraum abdeckt.
- `lib/api.ts`: typisierte Wrapper um `invoke`; ohne Tauri (`npm run dev` im Browser) wird
  `lib/mock.ts` verwendet, ein In-Memory-Backend mit zwei Teams und deterministischen Beispiel-Worklogs.
- Diagramme sind eigene SVG-Komponenten (`BarChart`, `HBars`, `Donut`).
