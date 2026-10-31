# Änderungen

## 0.3.0 – 2026-10-31

Vier Sicherungen gegen stille Lücken in den Zeiten:

- **Quellen-Abgleich** (Einstellungen → Verbindung): liest den angezeigten Zeitraum aus Jira und
  Tempo und stellt beide je Mitglied und Tag gegenüber, inklusive der Worklogs, die Jira anderen
  Autoren zuordnet, und der Tempo-Worklogs auf nicht lesbaren Tickets. Der Cache bleibt unverändert.
- **Stand** in der Kopfzeile zeigt jetzt den ältesten Abruf im Zeitraum statt des jüngsten. Ist er
  älter als das doppelte Aktualisierungsintervall (mindestens eine Stunde), wird das als Warnung mit
  „Aktualisieren empfohlen“ angezeigt; ein Klick lädt neu. Die automatische Aktualisierung richtet
  sich ebenfalls nach dem Alter der geladenen Tage, sodass ein Zeitraum nach dem App-Start
  nachgeladen wird, wenn er älter als das Intervall ist.
- Wird der Zeitraum oder das Team gewechselt, während ein Abruf läuft, holt die App die neue
  Ansicht nach dem laufenden Abruf automatisch nach (vorher blieb die Lücke bis zum nächsten Wechsel).
- Mitglieder ohne Zeiten werden nach jedem Abruf namentlich genannt: bei der Jira-Quelle zusammen
  mit den fremden Autoren, bei der Tempo-Quelle als Hinweis auf fehlende Buchungen oder fehlende
  Token-Berechtigung.

## 0.2.0 – 2026-10-31

- **Tempo Cloud als Quelle der Zeiten** (Einstellungen → Verbindung → Quelle der Zeiten). Bucht das
  Team in Tempo, führt Jira die Worklogs häufig unter dem Tempo-App-Konto; die Jira-API ordnet sie
  dann keiner Person zu und YaTTAT fand nur die direkt in Jira gebuchten Zeiten. Mit einem
  Tempo-API-Token liest YaTTAT die Worklogs jetzt aus der Tempo-API (echter Autor), löst die Tickets
  über Jira auf und wertet wie gewohnt aus. Beim Wechsel der Quelle wird der Cache geleert.
- Diagnose nach jedem Abruf: Worklogs im Zeitraum, die Jira anderen Autoren zuordnet (z. B. „Tempo
  Timesheets“), erscheinen in der Kopfzeile als Hinweis und unter Einstellungen → Info. Tempo-Worklogs
  auf in Jira nicht lesbaren Tickets werden gezählt.
- Fehlendes Tempo-Token bei gewählter Tempo-Quelle wird in der Kopfzeile angezeigt.

## 0.1.0 – 2026-10-31

- Erste Version: Zeiterfassungen (Worklogs) der Teammitglieder aus Jira (Cloud und Data Center)
  abrufen und auswerten. Anmeldung wie in YaTTT (Browser-Login mit SSO/2FA, Passwort, PAT, API-Token).
- Einstellungen: Jira-Verbindung, optionale Projektauswahl, **Teams und Mitglieder** (Suche nach
  Jira-Benutzern, Wochenstunden und Farbe je Mitglied, mehrere Teams), Soll-Arbeitszeit, Zeitformat,
  Erscheinungsbild, automatische Aktualisierung.
- Ansichten: Team (Kennzahlen, Stunden pro Mitglied mit Soll, Stunden pro Tag/Woche gestapelt nach
  Mitglied, Verteilung nach Projekt, Tickettyp und Wochentag, Mitgliedertabelle), Mitglied
  (Kennzahlen, Verlauf, Projekte, Top-Tickets, Worklog-Liste nach Tagen, CSV/JSON-Export),
  Tickets (Zeit pro Ticket mit Anteilen der Mitglieder, Schätzung und Jira-Gesamt, Filter,
  Gruppierung nach Epic/Projekt/Typ, Worklogs je Ticket).
- Lokaler SQLite-Cache mit Tagesabdeckung je Mitglied: nur fehlende Tage werden aus Jira geladen,
  „Aktualisieren“ lädt den angezeigten Zeitraum neu und entfernt gelöschte Worklogs.
- CI auf drei Plattformen und Release-Pipeline mit Installationspaketen für macOS, Windows und Linux.
