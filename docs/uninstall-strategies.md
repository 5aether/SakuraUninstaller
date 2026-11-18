# Strategie di disinstallazione

| Tipo | Comportamento |
| --- | --- |
| MSI | Il codice prodotto viene validato. Sakura avvia Windows Installer con `/x` e `/norestart`. La modalità silenziosa aggiunge `/qn`. |
| Inno Setup | Sakura privilegia il comando silenzioso registrato. In sua assenza, solo se il registro identifica Inno Setup, aggiunge `/VERYSILENT /SUPPRESSMSGBOXES /NORESTART` al comando ufficiale. |
| NSIS | Sakura privilegia il comando silenzioso registrato. In sua assenza, solo se il registro identifica NSIS, antepone `/S`, mantenendo gli argomenti originali e l’eventuale `_?=` finale. |
| Squirrel / Electron e altri EXE | Sakura riconosce gli indizi Squirrel disponibili e verifica il percorso del disinstallatore registrato. |
| MSIX | Sakura usa il gestore pacchetti di Windows per rimuovere il pacchetto rimovibile dell’utente corrente. |
| Portatile | Sakura parte dal file `.exe` indicato dall’utente e propone i residui attribuibili da esaminare. La cartella del programma resta al suo posto. |

I lavori in batch vengono eseguiti in sequenza. L’interfaccia mostra il programma corrente, la posizione nella coda e la fase: verifica e backup, disinstallazione, verifica della rimozione e ricerca dei residui. **Interrompi coda** impedisce l’avvio dei programmi successivi e lascia terminare l’operazione corrente, inclusi i controlli finali. Il rapporto indica quanti programmi non sono stati avviati. Gli annullamenti dell’utente e di UAC interrompono la coda e non vengono presentati come operazioni completate.

L’anteprima mostra il comando completo con gli argomenti o il pacchetto previsto, i processi rilevati e gli elementi da ricontrollare. I processi con finestre attribuibili al programma possono ricevere una richiesta di chiusura esplicita. Prima dell’avvio Sakura confronta anche cartella d’installazione e tipo di installer con la selezione confermata.

Il popup di disinstallazione è un pannello modale interno alla finestra e segue quattro fasi: disinstallazione, esame dei residui, pulizia e riepilogo. Mostra soltanto i residui delle applicazioni del batch corrente. Le sezioni espandibili **Registro** (chiavi e valori di avvio) e **File e cartelle** (cartelle e collegamenti) consentono la selezione individuale o per categoria. Servizi, attività e altri elementi non modificabili restano di sola lettura. Nessun residuo è selezionato automaticamente.

**Pulisci selezionati** conferma lo spostamento in quarantena degli elementi mostrati. Ogni elemento viene nuovamente verificato prima della rimozione e passa allo stato in corso, in quarantena o errore. La categoria dell’elemento corrente si apre durante la pulizia. Il pulsante di interruzione lascia terminare l’elemento corrente e mantiene disponibili quelli successivi. Il riepilogo conta solo i programmi di cui è stata verificata la rimozione e gli elementi effettivamente messi in quarantena, usando le dimensioni registrate al momento dello spostamento. Da **Esamina rimanenti** è possibile proseguire con gli elementi non rimossi. Le finestre eventualmente mostrate dal disinstallatore ufficiale rimangono gestite dal relativo programma; il popup di Sakura non le incorpora.

Dopo un esito positivo, Sakura ricontrolla l’inventario fino a 31 volte, a intervalli di 500 ms, per gestire i disinstallatori che delegano il lavoro a un altro processo. L’attesa può superare 15 secondi per il tempo necessario alle scansioni. La ricerca dei residui parte solo dopo la scomparsa della registrazione; per MSI viene controllata anche l’eventuale presenza dello stesso codice prodotto in un’altra voce. Una scansione MSIX non riuscita non conferma una rimozione e non cancella dalla cache i pacchetti già conosciuti.

Per MSI, `3010` segnala una disinstallazione riuscita che richiede un riavvio: i residui vengono proposti solo se la rimozione risulta già confermata. `1641` segnala un riavvio avviato e ferma la coda. `1614` richiede comunque la verifica dell’assenza del prodotto. `1618` interrompe la coda e invita ad attendere la conclusione dell’altra installazione. Questi codici MSI non vengono interpretati come esiti positivi per gli EXE generici.

Riferimenti: [parametri Inno Setup](https://jrsoftware.org/inno-setup/documentation/topic_uninstcmdline.htm), [parametri NSIS](https://nsis.sourceforge.io/Docs/Chapter3.html), [codici di Windows Installer](https://learn.microsoft.com/en-us/windows/win32/msi/error-codes).
