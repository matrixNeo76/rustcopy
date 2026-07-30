# 📖 OPERATIONAL RUNBOOK: robocopy-ingest-cli (rustcopy)

> **Manuale Operativo di Ingestion Massiva, Backup Incrementali Multi-Sorgente e Casi d'Uso REALI Verificati**  
> *Data: 30 Luglio 2026 | Versione: 5.1.0-Runbook | Stato: Documentazione Verificata su Share SMB Remota*

---

## 📌 1. Copia Multi-Sorgente verso la Stessa Destinazione

### ❓ È possibile copiare da più sorgenti diverse verso la stessa destinazione senza perdere l'allineamento incrementale?

**SÌ, assolutamente!**  
Esistono due modalità operative per gestire sorgenti multiple verso una destinazione comune (`\\FILESERV01\dati01\provarust`):

#### 🔹 Modalità A: Sorgenti Distinte in Sotto-cartelle (RACCOMANDATO)
Se vuoi consolidare più cartelle sorgente (es. `C:\repos`, `D:\projects`, `E:\docs`) dentro lo stesso repository di backup remoto, la buona norma è specificare una sotto-cartella dedicata in destinazione per ciascuna sorgente:
```powershell
# 1. Backup Sorgente A
.\target\release\robocopy_ingest.exe --source "C:\repos" --dest "\\FILESERV01\dati01\provarust\repos" --verify-integrity --hash-algo blake3

# 2. Backup Sorgente B
.\target\release\robocopy_ingest.exe --source "D:\projects" --dest "\\FILESERV01\dati01\provarust\projects" --verify-integrity --hash-algo blake3
```
- **Vantaggi**: Ogni sorgente mantiene la propria alberatura ed il proprio stato incrementale isolato. Non c'è alcun rischio di sovrascrittura o conflitto tra file con lo stesso nome provenienti da sorgenti diverse.

#### 🔹 Modalità B: Ingestion Multi-Sorgente nello Stesso Root (Merge Incremetale)
Se vuoi unire più sorgenti direttamente nel root della destinazione senza sotto-cartelle:
- **Copia Incrementale**: Robocopy confronterà ciascun file sorgente con il file corrispondente in destinazione. Se il file in destinazione non esiste o ha una data diversa, verrà aggiornato. Se è già identico, verrà saltato.
- **⚠️ REGOLA FONDAMENTALE (NON Usare `--mirror`)**: Quando si uniscono sorgenti multiple nello stesso root di destinazione, **NON si deve usare il flag `--mirror`**. Usando `--mirror` per la sorgente B, Robocopy cancellerebbe dalla destinazione i file precedentemente copiati dalla sorgente A!  
  *(Nota: Se si tenta di usare `--mirror`, la **Release 5.1.0** attiva la protezione `Mirror Safety Threshold` bloccando l'eliminazione accidentale).*

---

## 💻 2. Comandi Reali Eseguiti e Verificati con Successo

Di seguito sono riportati i comandi **realmente testati ed eseguiti sul campo con successo** (inclusi i benchmark di performance ed esito di integrità):

### 1. Ingestion Massiva Iniziale (55.314 File, 3.18 GB su SMB)
Esecuzione del trasferimento completo con verifica di integrità multi-core **BLAKE3**:
```powershell
.\target\release\robocopy_ingest.exe `
  --source "C:\Users\auresystem\repos" `
  --dest "\\FILESERV01\dati01\provarust" `
  --verify-integrity `
  --hash-algo blake3
```
- **Esito**: 55.314 file trasferiti su rete SMB a 17.35 MB/s costante in 3 minuti e 4s.

---

### 2. Aggiornamento Incrementale ad Alta Velocità (Filtro Log Attivo)
Esecuzione dell'aggiornamento incrementale con esclusione dei log in scrittura attiva:
```powershell
.\target\release\robocopy_ingest.exe `
  --source "C:\Users\auresystem\repos" `
  --dest "\\FILESERV01\dati01\provarust" `
  --exclude-files "robocopy_ingest.log" `
  --verify-integrity `
  --hash-algo blake3
```
- **Esito**: 55.269 file inalterati saltati a banda ZERO all'istante; 905 file modificati/nuovi trasferiti in soli **38 secondi**.

---

### 3. Simulazione Preventiva Dry-Run (Senza Modifiche ai Dati)
Test di verifica senza scrivere o alterare i dati di destinazione:
```powershell
.\target\release\robocopy_ingest.exe `
  --source "C:\Users\auresystem\repos" `
  --dest "\\FILESERV01\dati01\provarust" `
  --dry-run `
  --verify-integrity
```
- **Esito**: Generazione dell'inventario e simulazione completata in 1.75 secondi.

---

### 4. Backup Enterprise con Web Server di Stato (Porta 8080)
Avvio dell'ingestion con un server HTTP di stato visibile via browser su `http://localhost:8080`. **Nota**: `--serve-dashboard` espone al momento solo una pagina statica ("Status: ACTIVE"), non un dashboard con progresso in tempo reale — per il progresso vero usare la progress bar in console o il report JSON/HTML a fine job:
```powershell
.\target\release\robocopy_ingest.exe `
  --source "C:\Users\auresystem\repos" `
  --dest "\\FILESERV01\dati01\provarust" `
  --serve-dashboard 8080 `
  --html-report-path "\\FILESERV01\dati01\provarust\dashboard.html" `
  --verify-integrity `
  --hash-algo blake3
```

---

### 5. Disaster Recovery / Ripristino da Report JSON (Reverse Restore)
Ripristino guidato in caso di guasto del server principale partendo dal report JSON di backup:
```powershell
.\target\release\robocopy_ingest.exe `
  --restore-from "\\FILESERV01\dati01\provarust\robocopy_ingest_report.json"
```

---

## 📑 3. Indice Documentazione di Progetto

| Documento | Descrizione e Contenuto |
|---|---|
| 📘 **[README.md](file:///c:/Users/auresystem/repos/robocopy-ingest-cli/README.md)** | Guida generale, tabella flag CLI e panoramica di alto livello. |
| 📖 **[RUNBOOK.md](file:///c:/Users/auresystem/repos/robocopy-ingest-cli/RUNBOOK.md)** | **[QUESTO DOCUMENTO]** Guida operativa, backup multi-sorgente e comandi reali testati. |
| 📄 **[ARCHITECTURE.md](file:///c:/Users/auresystem/repos/robocopy-ingest-cli/ARCHITECTURE.md)** | Architettura interna v5.1.0, diagrammi di flusso e mappa dei moduli Rust. |
| 📊 **[ANALYSIS.md](file:///c:/Users/auresystem/repos/robocopy-ingest-cli/ANALYSIS.md)** | Diagnosi di robustezza, tuning 3x performance e 140 test di validazione. |
| 🗺️ **[ROADMAP.md](file:///c:/Users/auresystem/repos/robocopy-ingest-cli/ROADMAP.md)** | Diagramma Gantt dello storico delle release (v1.0 - v5.1). |
