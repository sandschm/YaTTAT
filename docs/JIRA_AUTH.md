# Jira access: login and required permissions

YaTTAT uses the same login as [YaTTT](https://github.com/sandschm/YaTTT): the default is the
**browser login** in its own window (SSO and a second factor work as in the browser, the session
cookies are taken over and stored encrypted). Alternatives: username and password (Data Center
without SSO), personal access token (Data Center) or e-mail and API token (Cloud). The mechanism was
confirmed in YaTTT against Jira Cloud with SSO; details and the test protocol are in YaTTT's
`docs/JIRA_AUTH.md`.

## What YaTTAT reads

| Purpose | Endpoint |
|---|---|
| Check the session, detect the deployment | `GET /rest/api/2/myself`, `GET /rest/api/2/serverInfo` |
| Search members (Settings → Team) | `GET /rest/api/2/user/search` (Cloud: `query`, Data Center: `username`) |
| Search projects | `GET /rest/api/2/project/search` (Cloud) or `GET /rest/api/2/project` (DC) |
| Issues with worklogs of the members in the range | `GET /rest/api/2/search/jql` (Cloud) or `GET /rest/api/2/search` (DC) with `worklogAuthor in (…) AND worklogDate >= … AND worklogDate <= …` |
| Worklogs per issue | `GET /rest/api/2/issue/{key}/worklog` (paginated) |

YaTTAT never writes to Jira. The signed-in person only sees worklogs on issues they may read in Jira;
times on issues in projects without access are missing from the analysis.

## Source Tempo Cloud

If the team logs time in Tempo, Tempo creates the Jira worklogs under the Tempo app account,
depending on the configuration. The Jira API then reports "Tempo Timesheets" as the author instead of
the person, and `worklogAuthor` does not find the issues. YaTTAT reports this after a fetch as
"Worklogs by other authors found". Remedy: Settings → Connection → Source of times → **Tempo Cloud**
with a Tempo API token (Tempo → Settings → API integration).

| Purpose | Endpoint |
|---|---|
| Check the token | `GET https://api.tempo.io/4/worklogs/user/{own accountId}?limit=1` |
| Worklogs per member in the range | `GET https://api.tempo.io/4/worklogs/user/{accountId}?from&to&offset&limit` (paginated) |
| Issues of the worklogs | `GET /rest/api/2/issue/{id}` per issue id (Jira session, 4 in parallel) |

The token is stored encrypted like the Jira credentials and needs the Tempo permission to see the
members' worklogs (team lead or "View all worklogs"). Tempo worklogs on issues the signed-in person
may not read in Jira are counted but not analysed.

## User ids

- Jira Cloud identifies people by `accountId`; e-mail addresses may be hidden depending on the
  privacy settings. The search needs at least one character.
- Jira Data Center uses the username (`name`); an empty search lists all active users.

The id is stored with the member and used for `worklogAuthor` and for matching worklog authors. If a
user is renamed in Jira, the id stays the same.

## Credential storage

- The operating system's keyring (macOS Keychain, Windows Credential Manager, Linux Secret Service)
  only holds a random 32-byte key (service `de.yattat.app`).
- Session cookies, tokens and, if used, the password are stored XChaCha20-Poly1305-encrypted in
  `secrets.enc` in the configuration directory.
- Without a keyring (e.g. Linux without a Secret Service) the key is kept in `secrets.key` with
  permissions 0600; the settings then show a hint.

## Limits

- Cookie sessions expire (Atlassian Guard can enforce short sessions); the app then shows "Sign in
  again".
- After several failed attempts Jira demands a CAPTCHA; the password login reports that.
- Corporate proxies and internal certificates: the Rust client uses the system certificate store and
  the proxy environment variables (`HTTPS_PROXY`).
