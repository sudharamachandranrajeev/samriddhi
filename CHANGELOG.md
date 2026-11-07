# Changelog

## [0.2.0] - 2026-11-07

This version merges in the features of the earlier Flutter prototype, `samridhi`, which is
now retired.

- **Wealth**: your accounts (ISA, pension, taxable, cash) and what is in them, added up by kind
  of asset. Holdings in another currency are listed but not counted until exchange rates
  arrive.
- **Plan**: a target mix of assets for each goal, from your risk level and how soon you need
  the money, blended into one overall target. Short goals are kept safer as their date nears.
  A goal with no date gets an assumed horizon, clearly marked. Optional tiers 1 to 7 give every
  goal the same mix, with a warning.
- **Insights**: rebalancing against the plan (with a new-money option that sells nothing), a
  next-year return forecast with its track record, a backtest of three rebalancing rules and a
  direct-indexing simulator. All are labelled as models on illustrative or made-up figures.
- **Suggestions on Home**: Samriddhi's own checks (risk profile, plan, rebalancing, outlook,
  tax wrappers, habits) add steps to the same list, each with what it rests on and how far to
  trust it. **Accept (note my choice)** only records your choice on this device; Samriddhi
  never trades, moves money or contacts anyone.
- **Import holdings** from a JSON file (`samriddhi-holdings-v1`, and the older
  `samridhi-holdings-v1`) or a CSV file (`account,wrapper,holding,assetClass,value` and an
  optional `currency`). You see a preview first; an import never deletes, and importing the
  same file again updates the same rows.
- **Optional AI, off by default**: a model on your own computer or network, any
  OpenAI-compatible cloud service, or Anthropic, with your own key kept in the system keyring
  (or the Android Keystore). You see the exact text before it is sent, and it holds only
  rounded, nameless figures.
- **Security status** in Settings: app lock, fingerprint or face, encryption and the last
  backup at a glance.
- Android: an optional switch to block screenshots, screen recording and the app-switcher
  preview, and a "rooted phone" row for information.
- Windows: a status row for the Brahma Sentinel security service, if it is installed.
- The wait after too many wrong PINs now survives closing and reopening the app.
- Accounts, holdings and your accepted steps are included in the full export and import.

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
