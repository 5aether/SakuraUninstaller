# Strategie di disinstallazione

| Tipo | Comportamento |
| --- | --- |
| MSI | Il codice prodotto viene validato. Sakura avvia Windows Installer con `/x` e `/norestart`. La modalità silenziosa aggiunge `/qn`. |
| Inno Setup e NSIS | Sakura usa le informazioni di riconoscimento disponibili nel registro e avvia il comando registrato. La modalità silenziosa usa il comando silenzioso registrato. |
| Squirrel / Electron e altri EXE | Sakura riconosce gli indizi Squirrel disponibili e verifica il percorso del disinstallatore registrato. |
| MSIX | Sakura usa il gestore pacchetti di Windows per rimuovere il pacchetto rimovibile dell’utente corrente. |
| Portatile | Sakura parte dal file `.exe` indicato dall’utente e propone i residui attribuibili da esaminare. La cartella del programma resta al suo posto. |

I lavori in batch vengono eseguiti in sequenza. L’anteprima mostra il comando o il pacchetto previsto, i processi rilevati e gli elementi da ricontrollare. I processi con finestre attribuibili al programma possono ricevere una richiesta di chiusura esplicita.
