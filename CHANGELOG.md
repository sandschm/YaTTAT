# Changelog

## 0.4.0 – 2026-10-31

- **English user interface.** Settings → Time & display → Language: System (automatic), Deutsch or
  English. Covers the whole interface, dates and decimal separators, CSV headers and the messages
  the backend sends (errors, login progress, status).
- All documents in the repository (README, changelog, docs, release notes) are in English; the
  screenshots show the English interface.
- App icon in the colours of the YaTTT icon (cyan and yellow).

## 0.3.0 – 2026-10-31

Four safeguards against silent gaps in the times:

- **Source comparison** (Settings → Connection): reads the displayed range from Jira and Tempo and
  compares both per member and day, including the worklogs Jira attributes to other authors and the
  Tempo worklogs on unreadable issues. The cache is not changed.
- **As of** in the top bar now shows the oldest fetch in the range instead of the newest. If it is
  older than twice the refresh interval (at least one hour), it is shown as a warning with "refresh
  recommended"; a click reloads. The automatic refresh also goes by the age of the loaded days, so a
  range is reloaded after the app starts when it is older than the interval.
- When the range or the team is changed while a fetch is running, the new view is fetched
  automatically after the running fetch (before, the gap stayed until the next change).
- Members without times are named after every fetch: with the Jira source together with the foreign
  authors, with the Tempo source as a hint at missing bookings or missing token permission.

## 0.2.0 – 2026-10-31

- **Tempo Cloud as source of times** (Settings → Connection → Source of times). If the team logs
  time in Tempo, Jira often lists the worklogs under the Tempo app account; the Jira API then cannot
  attribute them to a person and YaTTAT only found the times logged directly in Jira. With a Tempo
  API token YaTTAT now reads the worklogs from the Tempo API (real author), resolves the issues
  through Jira and analyses as usual. Switching the source clears the cache.
- Diagnostics after every fetch: worklogs in the range that Jira attributes to other authors (e.g.
  "Tempo Timesheets") appear as a hint in the top bar and under Settings → About. Tempo worklogs on
  issues that cannot be read in Jira are counted.
- A missing Tempo token with the Tempo source selected is reported in the top bar.

## 0.1.0 – 2026-10-31

- First version: fetch and analyse the time tracking (worklogs) of team members from Jira (Cloud
  and Data Center). Login as in YaTTT (browser login with SSO/2FA, password, PAT, API token).
- Settings: Jira connection, optional project selection, **teams and members** (search for Jira
  users, weekly hours and colour per member, several teams), target hours, time format, appearance,
  automatic refresh.
- Views: team (KPIs, hours per member with target, hours per day/week stacked by member,
  distribution by project, issue type and weekday, member table), member (KPIs, trend, projects, top
  issues, worklog list by day, CSV/JSON export), issues (time per issue with the members' shares,
  estimate and Jira total, filters, grouping by epic/project/type, worklogs per issue).
- Local SQLite cache with day coverage per member: only missing days are loaded from Jira,
  "Refresh" reloads the displayed range and removes deleted worklogs.
- CI on three platforms and a release pipeline with installers for macOS, Windows and Linux.
