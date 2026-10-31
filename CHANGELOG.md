# Änderungen

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
