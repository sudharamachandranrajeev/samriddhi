# Changelog

## [0.1.0] (unreleased)

- First foundation: an encrypted store on your device, a recovery key shown once, an optional
  PIN (and fingerprint or face unlock on Android), and a daily encrypted backup (the newest 14
  are kept).
- Money is kept exactly, in each currency's smallest unit, never as a rounded decimal.
- A short risk questionnaire in plain words: how much risk you are comfortable with, and how
  much loss you could take, kept apart.
- First "next best step" suggestions from simple, dated rules, each with the reason and where
  the facts came from.
- Full export of everything as JSON and CSV, and import from that export with a preview.
- The app itself: an eight-step first-run guide (welcome, protect your data, countries, goals,
  money, risk, updates, first step), each screen saying in one sentence why it asks.
- An unlock screen with PIN, fingerprint or face (Android), and the recovery key.
- A home screen with a rough net worth (cash minus debts), the next best step with **Later**,
  **Not for me** (with an optional reason) and **Show again**, an explanation on every figure
  and a short "Learn" hint on every screen.
- Settings for the look, the lock, backups (back up now, save a copy, a second folder, restore
  with a preview), export and import with a count preview, and changing your answers.
- Three looks, each in light and dark: Swarna (night navy and gold, the default), Bahi (cream
  ledger, red serif) and Vana (airy mint and periwinkle). Pages and themes blend into each
  other, unless your device asks for reduced motion or you pick calm motion.
- Keyboard use throughout, with visible focus, labelled controls and a skip link.
- Releases for Windows (installer and portable zip), Linux (AppImage and Arch package) and
  Android (APK), each with a SHA-256 checksum, built and signed by GitHub Actions.
- Updates you can undo: Samriddhi can check GitHub for a new version when it starts (or never),
  shows the size and what is new, and installs only when you press **Install and restart**.
  Every download is checked against the project's signature. **Later** always works.
- An encrypted backup before every update (the last three are kept, apart from the daily
  ones), and **Settings → Updates → Go back a version** on a computer. On Android, restore the
  backup made before the update.
- **Report a problem** and **Suggest a feature** open a prefilled GitHub issue with the version,
  system, install type, theme and the last three errors, with amounts, names, paths and numbers
  taken out.
