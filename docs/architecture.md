# Architettura

`src/main.rs` è il punto di ingresso del binario. `src/lib.rs` inizializza l’inventario, la finestra e Slint. `src/ui.rs` collega le callback allo stato in `src/state.rs` e aggiorna i modelli quando riceve il risultato di un lavoro in background.

| Percorso | Responsabilità |
| --- | --- |
| `src/scanner/` | Inventario Win32 e MSIX, misurazione delle cartelle, rilevamento e confronto dei residui, autorun, scorciatoie, servizi e attività pianificate |
| `src/uninstall/` | Coda dei lavori, comandi registrati, Windows Installer, MSIX, app portatili e processi |
| `src/safety/` | Regole e percorsi protetti, preflight, manifesti di backup, compressione della quarantena e ripristino |
| `src/storage/` | SQLite per inventario, misure e rapporti, migrazione JSON, preferenze e revisioni dei residui |
| `src/reporting/` | Registro delle attività, rapporti italiano/inglese e invio facoltativo dei contatori diagnostici |
| `src/utils/` | Percorsi Windows, stringhe, SHA-256 e WinHTTP |
| `src/updates.rs` | Verifica del canale firmato e download del pacchetto di aggiornamento |
| `ui/components/` | Scheda, elenco, conferma, avanzamento, pannello residui e rapporto |
| `packaging/` | Installer MSI WiX e script di firma |

`build.rs` compila la finestra e i suoi componenti Slint. La palette chiara e scura resta in `ui/sakura-theme.slint`. La libreria Material 3 inclusa è in `material-1.1.0/`.

I test usano i dati di `tests/fixtures/`. `tests/unit/` contiene i casi per il parsing e le firme. `tests/integration/` contiene i casi con registro e file temporanei e lo smoke test dell’eseguibile. Il workflow Windows è in `.github/workflows/ci.yml`.
