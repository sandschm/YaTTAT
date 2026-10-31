# Jira-Zugriff: Anmeldung und benötigte Rechte

YaTTAT verwendet dieselbe Anmeldung wie [YaTTT](https://github.com/sandschm/YaTTT): Standard ist die
**Browser-Anmeldung** in einem eigenen Fenster (SSO und zweiter Faktor funktionieren wie im Browser,
die Session-Cookies werden übernommen und verschlüsselt gespeichert). Alternativ Benutzername und
Passwort (Data Center ohne SSO), Personal Access Token (Data Center) oder E-Mail und API-Token
(Cloud). Der Mechanismus ist in YaTTT gegen Jira Cloud mit SSO bestätigt; Details und Testprotokoll
stehen dort unter `docs/JIRA_AUTH.md`.

## Was YaTTAT liest

| Zweck | Endpunkt |
|---|---|
| Sitzung prüfen, Deployment erkennen | `GET /rest/api/2/myself`, `GET /rest/api/2/serverInfo` |
| Mitglieder suchen (Einstellungen → Team) | `GET /rest/api/2/user/search` (Cloud: `query`, Data Center: `username`) |
| Projekte suchen | `GET /rest/api/2/project/search` (Cloud) bzw. `GET /rest/api/2/project` (DC) |
| Tickets mit Worklogs der Mitglieder im Zeitraum | `GET /rest/api/2/search/jql` (Cloud) bzw. `GET /rest/api/2/search` (DC) mit `worklogAuthor in (…) AND worklogDate >= … AND worklogDate <= …` |
| Worklogs je Ticket | `GET /rest/api/2/issue/{key}/worklog` (paginiert) |

YaTTAT schreibt nichts nach Jira. Die angemeldete Person sieht nur Worklogs von Tickets, auf die sie
in Jira Leserechte hat; Zeiten auf Tickets aus Projekten ohne Zugriff fehlen in der Auswertung.

## Benutzer-IDs

- Jira Cloud identifiziert Personen über die `accountId`; E-Mail-Adressen sind je nach
  Datenschutzeinstellung nicht sichtbar. Die Suche verlangt mindestens ein Zeichen.
- Jira Data Center verwendet den Benutzernamen (`name`); eine leere Suche listet alle aktiven Benutzer.

Die ID wird mit dem Mitglied gespeichert und für `worklogAuthor` sowie den Abgleich der
Worklog-Autoren verwendet. Wird ein Benutzer in Jira umbenannt, bleibt die ID gleich.

## Speicherung der Zugangsdaten

- Im Schlüsselbund des Betriebssystems (macOS Keychain, Windows Credential Manager, Linux Secret
  Service) liegt nur ein zufälliger 32-Byte-Schlüssel (Service `de.yattat.app`).
- Session-Cookies, Tokens und ggf. das Passwort liegen XChaCha20-Poly1305-verschlüsselt in
  `secrets.enc` im Konfigurationsverzeichnis.
- Ohne Schlüsselbund (z. B. Linux ohne Secret Service) liegt der Schlüssel in `secrets.key` mit
  Rechten 0600; die Einstellungen zeigen dann einen Hinweis.

## Grenzen

- Cookie-Sitzungen laufen ab (Atlassian Guard kann kurze Sitzungen erzwingen); die App zeigt dann
  „Erneut anmelden“.
- Nach mehreren Fehlversuchen verlangt Jira ein CAPTCHA; die Passwort-Anmeldung meldet das.
- Unternehmens-Proxys und interne Zertifikate: der Rust-Client nutzt den System-Zertifikatspeicher
  und die Proxy-Umgebungsvariablen (`HTTPS_PROXY`).
