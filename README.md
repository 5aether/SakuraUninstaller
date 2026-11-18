# Sakura Uninstaller

Uninstaller moderno, veloce e sicuro per Windows, scritto in Rust.

L’obiettivo è offrire una disinstallazione affidabile delle applicazioni, il rilevamento dei residui, la pulizia controllata e la possibilità di ripristinare le operazioni.

> **Principio fondamentale:** l’app non deve cancellare nulla “alla cieca”. Deve prima eseguire il disinstallatore ufficiale, poi rilevare i residui, mostrarli all’utente e consentire un ripristino.

Sakura Uninstaller legge le registrazioni Win32 di sistema e dell’utente e i pacchetti MSIX dell’utente corrente. Mostra editore, versione e dimensione dichiarata quando disponibile e permette di cercare e selezionare più programmi. L’interfaccia Material Design 3 segue il tema di Windows. La prima apertura usa l’italiano se è la lingua dell’interfaccia di Windows e altrimenti l’inglese. Il selettore salva la preferenza.

1. In **Software installato**, cerca per nome, editore o versione e seleziona i programmi.
2. Usa **Anteprima** e **Rapporti** per verificare il disinstallatore, i processi rilevabili e gli elementi che Sakura ricontrollerà. **Rapporti** consente di consultare anche le operazioni precedenti nelle due lingue. **Misura** somma le dimensioni dei file quando la cartella d’installazione è attribuibile. **Chiudi app** invia una richiesta di chiusura alle finestre dei programmi selezionati e lascia alle app la possibilità di chiedere il salvataggio.
3. Conferma **Disinstalla** per avviare in sequenza i disinstallatori registrati. L’avanzamento indica il programma e la fase corrente; **Interrompi coda** lascia terminare l’operazione corrente e ferma le successive. Puoi scegliere se cercare i residui dopo l’operazione. Per le app MSIX rimovibili, Sakura usa il gestore pacchetti di Windows per l’utente corrente. La modalità silenziosa per Win32 è disponibile per MSI, Inno Setup e NSIS identificati dal registro e per i programmi con un comando silenzioso registrato.
4. Il popup interno segue la disinstallazione e mostra i residui del batch separati in **Registro** e **File e cartelle**. Espandi le categorie, esamina i percorsi e seleziona gli elementi da pulire; **Pulisci selezionati** li sposta in quarantena. Il popup mostra l’avanzamento per elemento e un riepilogo delle rimozioni riuscite. Puoi interrompere la pulizia tra un elemento e il successivo. Le integrazioni non modificabili rimangono di sola lettura e tutti i residui restano consultabili anche nella scheda **Residui**. Sakura confronta le cartelle prima e dopo la disinstallazione, controlla che il programma non risulti più installato ed esclude percorsi in uso da altre installazioni, cartelle di sistema e punti di reparse.
5. In **Quarantena**, ripristina gli elementi nella posizione originale, se libera. La quarantena sposta i file sullo stesso volume e comprime i file idonei con il supporto nativo di Windows. Il ripristino verifica SHA-256 e prova a ripristinare gli attributi di compressione originali. Sui volumi privi di compressione i backup rimangono utilizzabili.
6. Usa **Aggiungi portatile** per indicare l’eseguibile di un’app in una cartella dedicata. Sakura tratta i dati attribuibili, le voci di avvio e le scorciatoie come residui da esaminare. La cartella del programma portatile resta disponibile.

Il registro delle attività, le revisioni e la quarantena si trovano in `%LOCALAPPDATA%\SakuraUninstaller`. Il ripristino riguarda i dati messi in quarantena. Per reinstallare un programma occorre il suo installer.

L’inventario mostra subito la cache locale quando disponibile. Per ogni chiave Win32 riusa i dati se la chiave ha lo stesso timestamp di modifica. Prima di avviare una disinstallazione, Sakura rilegge la registrazione. Le etichette MSI, MSIX, Inno Setup, NSIS ed Electron/Squirrel usano gli indizi disponibili. L’inventario mostra la data quando la registrazione Win32 la pubblica nel formato AAAAMMGG.

`sakura.sqlite3` conserva l’ultimo inventario, le misure delle installazioni e la cronologia dei rapporti. Al primo avvio migra l’inventario e i rapporti JSON precedenti, i cui file restano disponibili. Una misura è una fotografia della dimensione logica e decade se cambiano la registrazione o i metadati della cartella radice. Usa **Misura** per ricontrollare anche le modifiche ai singoli file nelle sottocartelle. I manifesti di quarantena sono leggibili separatamente dal database.

| Profilo | Elementi esaminati |
| --- | --- |
| Sicura | Cartelle locali attribuibili |
| Equilibrata | Cartelle, chiavi foglia attribuibili in HKCU, avvio per utente e collegamenti personali |
| Approfondita | Aggiunge voci HKLM, servizi Win32 e attività pianificate attribuibili |

Modifiche al registro limitate a chiavi foglia dell’utente e voci di avvio personali. Servizi, attività pianificate e riscontri HKLM sono di sola lettura. I dati MSIX restano gestiti da Windows. Sakura registra gli esiti su disco e conserva un backup dei valori personali individuati prima della disinstallazione. I percorsi non attribuibili richiedono un controllo manuale.

## Installer, firme e aggiornamenti

Per scaricare l’EXE compilato, apri **Actions → Windows CI → una run → Artifacts → SakuraUninstaller.exe**, oppure usa il collegamento **Download SakuraUninstaller.exe** nel riepilogo del job di build. L’EXE in modalità release viene caricato direttamente dopo il controllo di avvio, prima della preparazione dell’MSI, e resta disponibile per 14 giorni. Puoi avviare una build manuale con **Run workflow**. Il pacchetto completo con MSI, licenze e checksum rimane nell’artefatto `SakuraUninstaller-Windows-x64`.

Il workflow crea l’MSI per l’utente corrente e prova installazione, avvio e rimozione su un runner Windows isolato. Per firmare i tag, imposta i segreti GitHub `SAKURA_SIGNING_PFX` con il PFX in Base64 e `SAKURA_SIGNING_PASSWORD` con la password. Per gli aggiornamenti firmati, imposta anche la variabile `SAKURA_RELEASE_PUBLIC_KEY` con i 32 byte pubblici Ed25519 in esadecimale e il segreto `SAKURA_RELEASE_PRIVATE_KEY` con il PEM privato in Base64. Il workflow prepara `update.json` per la release e verifica che la chiave corrisponda a quella inclusa nella build.

Per attivare il canale, la release GitHub con tag `vX.Y.Z` deve contenere `SakuraUninstaller-x64.msi` e `update.json`. In **Impostazioni** puoi controllare gli aggiornamenti o abilitare il controllo all’avvio. Prima di aprire Windows Installer, Sakura rifiuta i downgrade, verifica la firma del manifesto e confronta il checksum dell’MSI.

Le regole aggiuntive possono essere salvate in `%LOCALAPPDATA%\SakuraUninstaller\rules.json` come oggetto JSON con `payload` testuale e `signature` Ed25519 esadecimale. Il payload contiene gli array `protected_paths` e `protected_integrations`. Sakura applica le esclusioni soltanto con una firma verificata dalla chiave della build.

## Diagnostica facoltativa

La raccolta dei contatori locali parte disattivata. Per abilitare la condivisione facoltativa in una build, imposta le variabili GitHub o di compilazione `SAKURA_TELEMETRY_HOST` con il solo hostname e `SAKURA_TELEMETRY_PATH` con il percorso assoluto dell’endpoint. Il servizio deve accettare un POST HTTPS con `{"schema":1,"completed":3,"errors":1}` e restituire uno stato 2xx. In **Impostazioni**, Sakura mostra l’host e invia i totali cumulativi solo quando la raccolta è attiva e l’utente seleziona **Invia contatori**. I dati inviati contengono la versione dello schema e i due totali. Il servizio può osservare l’indirizzo IP della richiesta.

---

## Piano di sviluppo

Le sezioni seguenti conservano la specifica del progetto. Lo stato effettivo delle funzioni e della distribuzione è descritto sopra.

### MVP

- Enumerazione delle applicazioni installate.
- Lettura delle informazioni da `Programmi e funzionalità`.
- Avvio del disinstallatore originale.
- Rilevamento dei processi attivi correlati.
- Conferma esplicita prima delle operazioni distruttive.
- Log dettagliato delle operazioni.
- Modalità dry-run.
- Interfaccia desktop moderna.

### Versione completa

- Disinstallazione batch.
- Modalità silenziosa quando supportata.
- Rilevamento residui su file system, Registry, startup e servizi.
- Quarantena e ripristino dei residui.
- Backup delle chiavi Registry.
- Calcolo della dimensione reale delle applicazioni.
- Report di pulizia.
- Profili di pulizia: sicuro, equilibrato e approfondito.
- Cache locale per scansioni rapide.
- Riconoscimento di MSI, Inno Setup, NSIS, Electron, MSIX e app portable.
- Aggiornamenti automatici e firma del binario.

---

## Stack tecnologico

| Area | Libreria / tecnologia | Scopo |
|---|---|---|
| Linguaggio | Rust | Performance, sicurezza della memoria, eseguibile nativo |
| API Windows | `windows-rs` | Registry, processi, servizi, shell, privilegi, UI |
| Registry | `windows-registry` | Lettura e modifica ergonomica del Registry |
| GUI | Slint 1.19 (`slint`, `slint-build`) | Interfaccia desktop dichiarativa compilata con Cargo |
| Stile | Slint Material Components 1.1 | Componenti Material Design 3 e palette Sakura |
| Processi | Tool Help e Win32 | Individuazione dei processi e richiesta di chiusura alle finestre |
| File system | `walkdir` | Scansione ricorsiva di directory |
| Parallelismo | `rayon` | Scansione e calcolo dimensioni in parallelo |
| Lavori in background | Thread Rust e ciclo eventi Slint | Scansioni e disinstallazioni fuori dal thread grafico |
| Serializzazione | `serde`, `serde_json` | Preferenze, manifesti e migrazioni |
| Database locale | `rusqlite` | Cache, cronologia e metadati |
| Logging | JSON Lines | Registro locale delle operazioni |
| Hash | `sha2` | Integrità dei backup e dei manifest |
| Compressione | `FSCTL_SET_COMPRESSION` | Compressione trasparente sul volume locale |
| Test | `cargo test`, `tempfile` | Test unitari e di integrazione |
| Packaging | WiX Toolset / MSI | Installatore Windows professionale |

---

## Perché queste librerie

### `windows-rs`

È la libreria principale per interagire con Windows da Rust. Fornisce binding per le API Windows, incluse COM, servizi di sistema, UI e API a basso livello.

Usala per:

- Leggere il Registry.
- Enumerare processi.
- Avviare processi con controlli di sicurezza.
- Gestire privilegi e UAC.
- Interagire con servizi Windows.
- Usare le API Windows Installer per i pacchetti MSI.

### `windows-registry`

Semplifica l’accesso al Registry. È utile per leggere le chiavi di disinstallazione e per esportare o ripristinare backup.

Le applicazioni tradizionali Windows registrano le informazioni di disinstallazione principalmente in:

```text
HKLM\SOFTWARE\Microsoft\Windows\CurrentVersion\Uninstall
HKLM\SOFTWARE\WOW6432Node\Microsoft\Windows\CurrentVersion\Uninstall
HKCU\SOFTWARE\Microsoft\Windows\CurrentVersion\Uninstall
```

### Slint

Slint definisce finestre, layout e componenti nei file `.slint`. `build.rs` compila `ui/app-window.slint` e registra la libreria locale `@material`. La UI mostra i dati tramite proprietà e modelli e inoltra le azioni dell’utente a Rust tramite callback.

Il backend Rust deve contenere tutta la logica sensibile:

- Scansione del sistema.
- Lettura del Registry.
- Avvio dei disinstallatori.
- Terminazione dei processi.
- Cancellazione dei file.
- Backup e ripristino.

### Material Design 3 e tema di Windows

L’interfaccia usa la libreria Slint Material Components inclusa in `material-1.1.0/`. Finestra e controlli condividono le varianti chiare e scure della palette viola in `ui/sakura-theme.slint` tramite `MaterialPalette`. [Documentazione Slint Material Components](https://material.slint.dev/getting-started/)

Il tema segue la modalità delle app di Windows e si aggiorna anche con la finestra aperta. In **Impostazioni → Personalizzazione → Colori**, l’opzione **Personalizzata** permette di scegliere la modalità predefinita delle app separatamente da quella di Windows.

### Processi Windows

Tool Help enumera i PID e Win32 legge il percorso degli eseguibili. Sakura usa il percorso per attribuire i processi a un’applicazione prima di disinstallarla.

### `walkdir` e `rayon`

`walkdir` attraversa le cartelle; `rayon` parallelizza il lavoro. Insieme permettono di calcolare dimensioni e trovare residui molto più velocemente rispetto a una scansione sequenziale.

### `rusqlite`

Il database conserva l’inventario, la data della scansione, le misure delle installazioni e i rapporti bilingui. I manifesti, le revisioni e le regole sono file locali leggibili con strumenti standard.

---

## Architettura

Il binario parte da `src/main.rs` e delega l’avvio a `src/lib.rs`. `src/ui.rs` collega le callback Slint allo stato e ai servizi Rust. L’albero seguente mostra la struttura del repository. [Architettura](docs/architecture.md) · [Sicurezza](docs/security.md) · [Strategie di disinstallazione](docs/uninstall-strategies.md)

```text
SakuraUninstaller/
├─ src/
│  ├─ main.rs
│  ├─ lib.rs
│  ├─ ui.rs               # Collegamento tra callback Slint e servizi Rust
│  ├─ state.rs
│  ├─ updates.rs
│  │
│  ├─ scanner/
│  │  ├─ mod.rs
│  │  ├─ installed_apps.rs
│  │  ├─ registry_scanner.rs
│  │  ├─ filesystem_scanner.rs
│  │  ├─ startup_scanner.rs
│  │  ├─ service_scanner.rs
│  │  └─ leftovers.rs
│  │
│  ├─ uninstall/
│  │  ├─ mod.rs
│  │  ├─ runner.rs
│  │  ├─ msi.rs
│  │  ├─ inno_setup.rs
│  │  ├─ nsis.rs
│  │  ├─ msix.rs
│  │  ├─ portable.rs
│  │  ├─ processes.rs
│  │  └─ elevation.rs
│  │
│  ├─ safety/
│  │  ├─ mod.rs
│  │  ├─ backup.rs
│  │  ├─ compression.rs
│  │  ├─ quarantine.rs
│  │  ├─ restore.rs
│  │  ├─ rules.rs
│  │  └─ protected_paths.rs
│  │
│  ├─ storage/
│  │  ├─ mod.rs
│  │  ├─ database.rs
│  │  ├─ cache.rs
│  │  └─ migrations.rs
│  │
│  ├─ reporting/
│  │  ├─ mod.rs
│  │  ├─ audit_log.rs
│  │  ├─ diagnostics.rs
│  │  └─ report.rs
│  │
│  └─ utils/
│     ├─ mod.rs
│     ├─ paths.rs
│     ├─ http.rs
│     ├─ strings.rs
│     └─ hashing.rs
│
├─ ui/
│  ├─ app-window.slint
│  ├─ icons/
│  ├─ sakura-theme.slint
│  └─ components/
│     ├─ app-card.slint
│     ├─ app-list.slint
│     ├─ confirm-dialog.slint
│     ├─ progress-panel.slint
│     ├─ leftover-panel.slint
│     └─ report-panel.slint
│
├─ material-1.1.0/       # Componenti Material 3 inclusi nel repository
│
├─ tests/
│  ├─ unit/
│  │  ├─ command_lines.rs
│  │  ├─ diagnostics.rs
│  │  └─ update_signatures.rs
│  ├─ integration/
│  │  ├─ quarantine.rs
│  │  ├─ registry_inventory.rs
│  │  ├─ startup_inventory.rs
│  │  └─ windows_smoke.rs
│  └─ fixtures/
│     └─ update-payload.json
│
├─ docs/
│  ├─ architecture.md
│  ├─ security.md
│  └─ uninstall-strategies.md
│
├─ .github/
│  └─ workflows/
│     └─ ci.yml
│
├─ packaging/            # Installer MSI e script di firma
├─ build.rs
├─ Cargo.toml
├─ Cargo.lock
├─ README.md
└─ LICENSE
```

---

## Flusso di disinstallazione

```text
1. L’utente apre l’app
2. L’app carica la cache delle applicazioni
3. In background, aggiorna l’elenco dal Registry
4. L’utente seleziona una o più applicazioni
5. L’app mostra dimensioni, processi attivi e residui noti
6. L’utente sceglie la modalità:
   - Solo disinstallazione
   - Disinstallazione + pulizia residui
   - Dry run
7. L’app crea un backup delle chiavi Registry interessate
8. L’app chiude o segnala i processi attivi
9. L’app esegue il disinstallatore ufficiale
10. L’app attende il completamento
11. L’app riscansiona Registry, cartelle e scorciatoie
12. L’app mostra i residui trovati
13. L’utente conferma la pulizia
14. I file vengono spostati in quarantena
15. L’app genera un report
16. L’utente può ripristinare l’operazione
```

---

## Modello dati

### Applicazione installata

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InstalledApp {
    pub id: String,
    pub display_name: String,
    pub publisher: Option<String>,
    pub version: Option<String>,
    pub install_date: Option<String>,
    pub install_location: Option<PathBuf>,
    pub uninstall_string: Option<String>,
    pub quiet_uninstall_string: Option<String>,
    pub estimated_size_kb: Option<u64>,
    pub real_size_bytes: Option<u64>,
    pub installer_kind: InstallerKind,
    pub is_system_component: bool,
    pub registry_path: String,
    pub icon_path: Option<PathBuf>,
}
```

### Tipo di installer

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum InstallerKind {
    Msi,
    InnoSetup,
    Nsis,
    Msix,
    Squirrel,
    Electron,
    Portable,
    Unknown,
}
```

### Residuo rilevato

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Leftover {
    pub id: String,
    pub app_id: String,
    pub kind: LeftoverKind,
    pub path: Option<PathBuf>,
    pub registry_key: Option<String>,
    pub confidence: Confidence,
    pub size_bytes: Option<u64>,
    pub description: String,
    pub selected_by_default: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum LeftoverKind {
    Folder,
    File,
    RegistryKey,
    RegistryValue,
    StartupEntry,
    Service,
    ScheduledTask,
    Shortcut,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Confidence {
    High,
    Medium,
    Low,
}
```

---

## Collegamento tra Slint e Rust

Slint dichiara le azioni dell’utente nei file `.slint`. Rust registra i gestori tramite i metodi generati da Slint. La selezione usa gli identificatori delle registrazioni, così la ricerca non cambia l’identità dei programmi selezionati. La vista riceve i risultati dal worker tramite il ciclo eventi di Slint.

I gestori delle operazioni reali invocheranno questi servizi Rust:

| Azione nella UI | Servizio Rust previsto |
|---|---|
| Carica o aggiorna l’elenco | `list_installed_apps` |
| Mostra i dettagli di un’app | `inspect_app` |
| Conferma la disinstallazione | `uninstall_app` |
| Cerca i residui | `scan_leftovers` |
| Conferma la pulizia dei residui selezionati | `clean_leftovers` |
| Ripristina un’operazione | `restore_last_operation` |

Per le azioni che modificano il sistema, Rust deve verificare la conferma e applicare le regole di sicurezza. Scansioni e disinstallazioni vanno eseguite in background; modelli, progressi ed errori vanno aggiornati nel ciclo eventi di Slint.

---

## Esempio: lettura delle applicazioni

```rust
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InstalledApp {
    pub display_name: String,
    pub publisher: Option<String>,
    pub version: Option<String>,
    pub install_location: Option<PathBuf>,
    pub uninstall_string: Option<String>,
    pub quiet_uninstall_string: Option<String>,
    pub estimated_size_kb: Option<u64>,
    pub is_system_component: bool,
    pub registry_path: String,
}
```

La scansione deve leggere queste chiavi:

```text
HKLM\SOFTWARE\Microsoft\Windows\CurrentVersion\Uninstall
HKLM\SOFTWARE\WOW6432Node\Microsoft\Windows\CurrentVersion\Uninstall
HKCU\SOFTWARE\Microsoft\Windows\CurrentVersion\Uninstall
```

Valori da leggere:

```text
DisplayName
DisplayVersion
Publisher
InstallDate
InstallLocation
UninstallString
QuietUninstallString
EstimatedSize
SystemComponent
NoRemove
WindowsInstaller
```

Regole importanti:

- Ignora le voci senza `DisplayName`.
- Contrassegna le voci con `SystemComponent = 1`.
- Non mostrare come rimovibili le voci con `NoRemove = 1`.
- Non fidarti ciecamente di `EstimatedSize`: ricalcola la dimensione quando possibile.
- Normalizza i percorsi e gestisci correttamente le variabili d’ambiente.

---

## Strategie di disinstallazione

### MSI

Per i prodotti MSI, usa le API Windows Installer invece di interpretare manualmente la command line.

Microsoft documenta `MsiConfigureProduct` come funzione per installare o rimuovere un prodotto.

```text
MsiConfigureProduct(product_code, install_level, INSTALLSTATE_ABSENT)
```

Vantaggi:

- Maggiore affidabilità.
- Gestione corretta dei componenti MSI.
- Minore rischio di parsing errato della command line.

### Inno Setup

Cerca flag come:

```text
/SILENT
/VERYSILENT
/SUPPRESSMSGBOXES
/NORESTART
```

Non assumere che siano supportati: verifica il tipo di installer e, in caso di dubbio, usa la disinstallazione interattiva.

### NSIS

Alcuni installer NSIS supportano:

```text
/S
/SILENT
/NCRC
```

Anche in questo caso, verifica prima di usare la modalità silenziosa.

### MSIX

Non trattare le app MSIX come normali applicazioni Win32. Usa le API appropriate per Windows App SDK / AppX / MSIX.

### Portable

Per le applicazioni portable:

- Non eseguire un disinstallatore.
- Mostra chiaramente che non esiste una disinstallazione registrata.
- Proponi solo la rimozione dei dati utente, delle scorciatoie e delle voci di avvio.
- Richiedi conferma esplicita.

---

## Sicurezza

### Regole non negoziabili

- Non cancellare mai file senza conferma dell’utente.
- Non modificare Registry senza backup.
- Non eseguire l’intera GUI con privilegi amministrativi.
- Usa un processo helper separato per le operazioni che richiedono elevazione.
- Blocca percorsi di sistema critici.
- Non eliminare file condivisi o runtime senza verifica.
- Non fidarti dei nomi delle cartelle: verifica anche editore, percorso, eseguibile e GUID.
- Registra ogni azione in un log.
- Offri sempre una modalità dry-run.

### Percorsi da proteggere

Questi percorsi non devono mai essere cancellati automaticamente:

```text
C:\Windows
C:\Windows\System32
C:\Windows\SysWOW64
C:\Program Files\WindowsApps
C:\ProgramData\Microsoft\Windows
HKLM\SOFTWARE\Microsoft\Windows
HKLM\SYSTEM\CurrentControlSet
```

### Whitelist

Mantieni una whitelist di percorsi e chiavi protette, aggiornabile tramite file firmati.

Esempio:

```json
{
  "protected_paths": [
    "C:\\Windows",
    "C:\\Program Files\\WindowsApps"
  ],
  "protected_registry_keys": [
    "HKLM\\SOFTWARE\\Microsoft\\Windows",
    "HKLM\\SYSTEM\\CurrentControlSet"
  ],
  "protected_processes": [
    "explorer.exe",
    "svchost.exe",
    "csrss.exe",
    "winlogon.exe"
  ]
}
```

---

## Backup e ripristino

Prima della pulizia, crea una cartella operativa:

```text
%LocalAppData%\RustUninstaller\
├─ operations/
│  └─ 8f1c2b90/
│     ├─ manifest.json
│     ├─ registry-backup.reg
│     ├─ quarantine/
│     └─ logs/
└─ cache/
```

Il `manifest.json` deve contenere:

```json
{
  "operation_id": "8f1c2b90",
  "created_at": "2026-11-16T17:20:00Z",
  "app_name": "Esempio App",
  "status": "completed",
  "files": [],
  "registry_keys": [],
  "processes_killed": [],
  "commands_executed": [],
  "restore_available": true
}
```

Il ripristino deve:

1. Leggere il manifest.
2. Verificare gli hash.
3. Ripristinare le chiavi Registry dal backup.
4. Ripristinare i file dalla quarantena.
5. Registrare l’operazione di ripristino.

---

## Performance

### Scansione parallela

Usa `rayon` per:

- Calcolo delle dimensioni delle cartelle.
- Ricerca dei residui.
- Hash dei file quando necessario.
- Analisi delle scorciatoie.

### Cache

SQLite conserva nome e percorso di installazione, dimensioni misurate, data dell’ultima scansione e rapporti operativi. Le revisioni dei residui e gli hash di recupero sono in file separati. Il programma mostra prima l’inventario in cache e aggiorna le registrazioni in background.

### Operazioni in background

Il worker Rust esegue le operazioni in sequenza e trasferisce gli esiti al ciclo eventi di Slint. L’elaborazione in parallelo dei file avviene con Rayon dove applicabile.

---

## Qualità e testing

### Test obbligatori

- Parser delle chiavi Registry.
- Parser di `UninstallString`.
- Riconoscimento del tipo di installer.
- Normalizzazione dei nomi delle applicazioni.
- Matching dei residui.
- Regole di protezione dei percorsi.
- Backup e ripristino del Registry.
- Quarantena e ripristino dei file.
- Gestione dei processi.
- Gestori Rust delle callback Slint e aggiornamento dei modelli.

### Test di integrazione

Esegui i test in una macchina virtuale Windows con applicazioni finte:

- App MSI.
- App Inno Setup.
- App NSIS.
- App Electron.
- App portable.
- App con servizi.
- App con startup entry.
- App con residui nel Registry.

Non eseguire mai test distruttivi sul PC di sviluppo reale.

---

## CI/CD

Il workflow [Windows CI](.github/workflows/ci.yml) esegue `rustfmt`, Clippy e i test con Rust stabile e con la versione minima dichiarata. I test usano chiavi temporanee del registro e cartelle di prova. Dopo i test, il workflow costruisce la versione Windows x64, esegue uno smoke test in sola lettura, verifica il ciclo dell’MSI sul runner e carica i pacchetti con le licenze e i checksum SHA-256.

---

## Roadmap

### Fase 1 — MVP

- Lettura delle app installate.
- UI con ricerca e filtri.
- Disinstallazione tramite `UninstallString`.
- Conferma utente.
- Log delle operazioni.
- Dry-run.

### Fase 2 — Sicurezza

- Backup Registry.
- Quarantena file.
- Ripristino operazione.
- Whitelist percorsi protetti.
- Rilevamento processi attivi.
- Report di disinstallazione.

### Fase 3 — Residui

- Scansione file system.
- Scansione Registry.
- Startup entries.
- Servizi.
- Scheduled Tasks.
- Scorciatoie.
- Punteggio di confidenza.

### Fase 4 — Potenza

- Batch uninstall.
- Modalità silenziose.
- Supporto MSI avanzato.
- Supporto MSIX.
- Cache e scansione incrementale.
- Regole di residui firmate.
- Profili di pulizia.

### Fase 5 — Prodotto

- Aggiornamenti automatici.
- Firma del codice.
- Installatore MSI.
- Telemetria opt-in.
- Internazionalizzazione.
- Documentazione utente.

---

## Avvio locale

Su Windows servono Rust 1.95 o successivo con toolchain MSVC, gli strumenti di compilazione C++ di Visual Studio e il Windows SDK. Dalla radice del repository:

```powershell
cargo run --locked
cargo build --locked --release
```

Il primo comando avvia Sakura Uninstaller. Il secondo produce l’eseguibile in `target/release/SakuraUninstaller.exe`.

---

## Principio di design

L’app deve essere:

- **Veloce**, grazie a Rust, cache e parallelismo.
- **Sicura**, grazie a backup, conferme, whitelist e privilegi minimi.
- **Trasparente**, grazie a log, report e spiegazioni dei residui.
- **Reversibile**, grazie a quarantena e ripristino.
- **Moderna**, grazie a Slint e ai componenti Material Design 3.
- **Estensibile**, grazie a regole, plugin e moduli separati.

La qualità non è data dalla quantità di file cancellati, ma dalla capacità di rimuovere solo ciò che è davvero sicuro rimuovere.
