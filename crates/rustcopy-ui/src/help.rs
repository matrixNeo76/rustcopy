//! The text of the Aiuto page (CATALOGO_COMPORTAMENTI_GUI.md L28). The sentences about what the console
//! does live here; the meaning of an exit code is not written again: it comes from
//! `runner::exit_code_meaning`, the one place that knows it.
//!
//! It is written as a guide, not as a list of labels: what the program is for, how to start, what each
//! page is for, what to do when something goes wrong, then the words and the numbers. Every button it
//! names is spelled exactly as it is on screen, and a test keeps the exit codes in step with the core.

use robocopy_ingest::runner;

/// One line of the help: a heading, or a term with its explanation.
pub struct HelpEntry {
    pub heading: bool,
    pub term: String,
    pub text: String,
}

fn heading(title: &str) -> HelpEntry {
    HelpEntry {
        heading: true,
        term: title.to_string(),
        text: String::new(),
    }
}

fn item(term: &str, text: &str) -> HelpEntry {
    HelpEntry {
        heading: false,
        term: term.to_string(),
        text: text.to_string(),
    }
}

/// Every exit code the CLI can end with.
const EXIT_CODES: std::ops::RangeInclusive<u8> = 0..=7;

pub fn entries() -> Vec<HelpEntry> {
    let mut list = vec![
        heading("Che cos'è rustcopy"),
        item(
            "A cosa serve",
            "Copia cartelle da un posto a un altro (un disco, una chiavetta, una cartella di rete) in modo affidabile: copia solo ciò che è nuovo o cambiato, riprova se un file è occupato, può controllare che le copie siano identiche agli originali e tiene un report di ogni copia. Sotto usa Robocopy di Windows.",
        ),
        item(
            "Copia e attività",
            "Una copia è una singola esecuzione, e compare nell'elenco «Lavori» a sinistra. Un'attività è una copia che hai salvato con le sue cartelle e le sue impostazioni, per rilanciarla quando vuoi o pianificarla.",
        ),
        item(
            "Che cosa non fa mai da sola",
            "Non cancella nulla senza che tu lo veda e lo confermi. Il mirror (che rende la destinazione identica all'origine e quindi cancella) non si può accendere dalla console.",
        ),
        heading("Per cominciare"),
        item(
            "1. Crea un'attività",
            "Premi «Nuova attività» e segui i cinque passi: cosa copiare (un nome e le cartelle o i file), dove metterla e che tipo di copia, quando (a mano o pianificata), come (verifica, esclusioni, filtri) e il riepilogo. Alla fine «Crea attività» la scrive; da quel momento la trovi in «Attività» e la lanci con «Esegui».",
        ),
        item(
            "Oppure una copia una tantum",
            "«Copia al volo»: scegli o trascina le cartelle, scegli dove metterle e premi «Copia». Se la destinazione esiste già, vengono copiati solo i file nuovi o cambiati. Dal dettaglio, «Salva come attività...» la trasforma in un'attività.",
        ),
        item(
            "2. Controllare prima di copiare (copia al volo)",
            "«Controlla prima» conta file e dimensione di ciò che hai scelto e dice se la destinazione ha spazio. Lo fa solo quando glielo chiedi: su un disco grande può richiedere del tempo.",
        ),
        item(
            "3. Seguire la copia",
            "Mentre copia vedi la barra, i file e i MB al secondo, il grafico della velocità e gli ultimi file copiati. «Pausa» la sospende, «Ferma» la interrompe lasciando un punto da cui riprendere.",
        ),
        item(
            "4. Vedere com'è andata",
            "Quando finisce, la copia compare in «Lavori» con il suo esito. Aprila: vedi quanti file sono stati copiati, la durata e la velocità. «Mostra i dettagli tecnici» mostra il report completo.",
        ),
        item(
            "Se non hai niente da provare",
            "In questa pagina, «Crea un esempio in Documenti» prepara pochi file finti e un'attività già pronta. Non tocca dati veri.",
        ),
        heading("Le pagine, una per una"),
        item(
            "Copia al volo",
            "Dove si imposta e si avvia una copia: cartelle, destinazione, verifica dopo la copia e, se la destinazione è una chiavetta, «Espelli l'unità alla fine».",
        ),
        item(
            "Nuova attività",
            "La procedura guidata: «Avanti» e «Indietro» si muovono fra i passi senza perdere ciò che hai scritto, «Annulla» esce senza creare nulla. Un'attività incrementale o differenziale parte da una copia completa la prima volta. L'attività creata non fa mai mirror né cancellazioni e non ha comandi prima o dopo la copia.",
        ),
        item(
            "Attività",
            "È la pagina da cui si parte. Una riga per attività, con il nome e le cartelle, il tipo di copia, quando è pianificata, com'è andata l'ultima volta e quando sarà la prossima. Per ognuna: «Esegui» la rilancia, «Proprietà» mostra le impostazioni che usa davvero, «Modifica» prepara una proposta di modifica, «Storico» elenca le esecuzioni passate, «Pianifica» la fa partire all'ora scelta, «Togli» la toglie dall'elenco (senza cancellare il suo file). Qui puoi anche aggiungere un file di configurazione che hai già, aprire un report fatto altrove e gestire chiavi e password.",
        ),
        item(
            "Lavori (a sinistra)",
            "L'elenco delle copie fatte da questa console, con data, numero di file, dimensione ed esito. Un clic apre il dettaglio.",
        ),
        item(
            "Il dettaglio di una copia",
            "Esito, numeri e pulsanti: «Riprendi» (se è stata interrotta), «Ripeti», «Salva come attività...», «Mostra i dettagli tecnici», «Registro tecnico...», «Tutti i problemi...» (l'elenco dei file che non si sono copiati, esportabile in CSV) e «Anteprima ripristino» (mostra cosa tornerebbe indietro, senza copiare nulla).",
        ),
        item(
            "Proprietà",
            "Ogni impostazione di un'attività, raggruppata, con da dove viene il valore che vince e quali scelte hanno una conseguenza. Si legge soltanto.",
        ),
        item(
            "Modifica",
            "Scrive una proposta in un file nuovo accanto all'originale: quello in uso non viene mai toccato, non si può accendere il mirror e non si può abbassare la conservazione. La sostituzione la fai tu. In «Avanzate» si possono scrivere un comando prima e uno dopo la copia: solo con il livello Esperto, e dopo una conferma che ti mostra il testo esatto.",
        ),
        item(
            "Storico",
            "Le esecuzioni passate con il loro esito, un filtro per esito, l'esportazione in CSV e le osservazioni che il motore ricava dalle esecuzioni (per esempio una copia insolitamente lenta).",
        ),
        item(
            "Pianifica",
            "Prepara un'attività di Utilità di pianificazione di Windows che avvia la copia ogni giorno, ogni tot ore o nei giorni scelti, con il tuo utente e solo quando sei connesso. Serve il livello di sicurezza Standard.",
        ),
        item(
            "Chiavi e password",
            "Salva o toglie una chiave di cifratura o una password (per esempio quella della posta) in Gestione credenziali di Windows. Nei campi che la accettano si scrive keyring:NOME, mai il segreto.",
        ),
        item(
            "Sicurezza",
            "Il pulsante in fondo a sinistra mostra il livello in uso. La pagina dice, per ogni livello, che cosa si può e che cosa non si può fare. Alzarlo chiede una conferma, abbassarlo no.",
        ),
        heading("Quando qualcosa non va"),
        item(
            "«Da controllare»",
            "La copia è finita ma qualche file non si è copiato o non coincide. Apri il dettaglio e usa «Tutti i problemi...» per vedere quali e perché.",
        ),
        item(
            "«Interrotta»",
            "La copia si è fermata (l'hai fermata tu o il computer si è spento). Se ha lasciato un punto di ripresa compare «Riprendi»: i file già copiati non si ricopiano. Se ne ha lasciati più d'uno, scegli da quale riprendere.",
        ),
        item(
            "Il motivo di un errore",
            "Sta nel «Registro tecnico...» del dettaglio: il diario che rustcopy scrive mentre lavora. Il numero d'uscita ti dice solo che qualcosa è andato storto, il registro dice cosa. Le ultime righe sono quelle che servono.",
        ),
        item(
            "La copia è lenta",
            "Con tante cartelle e file piccoli, soprattutto verso una cartella di rete, il tempo lo fa il lavoro per ogni singolo file (aprirlo, crearlo, chiuderlo) più che la velocità del collegamento: 30.000 file da 25 KB sono molto più lenti di un solo file da 750 MB. È normale. La seconda copia è molto più veloce, perché copia solo ciò che è cambiato.",
        ),
        item(
            "Spazio insufficiente",
            "Prima di copiare rustcopy controlla che la destinazione abbia spazio. Se non basta si ferma subito, senza copiare nulla (codice 6): libera spazio o scegli un'altra destinazione.",
        ),
        item(
            "Un file è aperto da un altro programma",
            "Robocopy riprova alcune volte. Se il file resta occupato finisce tra i problemi. Per copiare anche i file aperti serve una pianificazione con privilegi elevati (livello Esperto), che usa una copia ombra del disco.",
        ),
        heading("Livelli di sicurezza"),
        item(
            "Prudente",
            "Il livello di partenza: avviare copie, vedere report e storico, salvare attività. Nessuna pianificazione e nessuna cancellazione.",
        ),
        item(
            "Standard",
            "In più: pianificazioni, «Sposta» (copia, verifica e cancella gli originali solo dopo la tua conferma) e «Forza completo».",
        ),
        item(
            "Esperto",
            "In più: pianificazioni con privilegi elevati, per copiare anche i file aperti, e i comandi prima e dopo la copia nell'editor (con conferma del testo). Mirror e pulizie non partono mai in automatico dalla console, a nessun livello.",
        ),
        heading("Parole che si incontrano"),
        item(
            "report",
            "Il file (.json) che racconta una copia: cosa è stato copiato, quanto ci ha messo, l'esito. Ogni copia ne scrive uno.",
        ),
        item(
            "registro tecnico",
            "Il diario di ciò che rustcopy fa mentre lavora, riga per riga. Serve soprattutto quando una copia fallisce.",
        ),
        item(
            "verifica",
            "Rilegge i file copiati e li confronta con gli originali. Più sicura, più lenta. xxh3 scopre gli errori di copia ma non è crittografico: per dati che qualcuno potrebbe manomettere scegli SHA-256 o BLAKE3.",
        ),
        item(
            "mirror",
            "Rende la destinazione identica all'origine e quindi cancella lì ciò che nell'origine non c'è più. È l'impostazione più distruttiva: la console non può accenderla, va scritta a mano nel file di configurazione.",
        ),
        item(
            "generazione, ciclo",
            "Con un tipo di copia scelto (completa, incrementale, differenziale) la copia diventa una storia di versioni: una completa più le copie che la seguono formano un ciclo. La conservazione toglie cicli interi, mai una generazione sola.",
        ),
        item(
            "pianificazione",
            "Un'attività di Utilità di pianificazione di Windows che avvia una copia a un'ora fissa.",
        ),
        item(
            "copia ombra (VSS)",
            "Un'istantanea del disco presa a inizio copia: permette di copiare anche i file che altri programmi hanno aperti. Richiede i diritti di amministratore.",
        ),
        item(
            "file di configurazione",
            "Un file .toml che descrive una o più copie (cartelle, impostazioni). Un'attività è, dietro le quinte, un file di questo tipo.",
        ),
        item(
            "ereditato",
            "In un file con più job, un valore non scritto nel job viene dai valori di primo livello. «Proprietà» dice per ogni voce da dove arriva.",
        ),
        heading("Come può finire una copia"),
        item(
            "Il numero d'uscita",
            "Quando rustcopy finisce restituisce un numero: lo vedi nello storico e lo legge Utilità di pianificazione. Zero vuol dire che è andato tutto bene.",
        ),
    ];
    for code in EXIT_CODES {
        list.push(item(&code.to_string(), runner::exit_code_meaning(code)));
    }
    list
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_entry_says_something_and_every_exit_code_is_there() {
        let list = entries();
        assert!(list.iter().all(|e| !e.term.trim().is_empty()));
        assert!(list
            .iter()
            .filter(|e| !e.heading)
            .all(|e| !e.text.trim().is_empty()));
        for code in EXIT_CODES {
            assert!(
                list.iter()
                    .any(|e| !e.heading && e.term == code.to_string()),
                "exit code {code} is missing"
            );
        }
    }

    /// The help lists every code the core defines: a new one added there must show up here. The core's
    /// own words for an unknown code is "sconosciuto", so none of the listed codes may read that way.
    #[test]
    fn no_listed_exit_code_is_unknown_to_the_core() {
        assert_eq!(*EXIT_CODES.end(), runner::EXIT_DESTINATION_UNSAFE);
        for code in EXIT_CODES {
            assert_ne!(
                runner::exit_code_meaning(code),
                "sconosciuto",
                "code {code}"
            );
        }
    }

    #[test]
    fn the_meaning_of_a_code_is_the_cores_not_a_copy() {
        let list = entries();
        let four = list.iter().find(|e| e.term == "4").expect("code 4");
        assert_eq!(four.text, runner::exit_code_meaning(4));
    }

    /// A guide, not a list: it has sections, and the first one says what the program is for.
    #[test]
    fn it_is_organised_in_sections_that_start_with_what_the_program_is_for() {
        let list = entries();
        let headings: Vec<&str> = list
            .iter()
            .filter(|e| e.heading)
            .map(|e| e.term.as_str())
            .collect();
        assert!(headings.len() >= 6, "{headings:?}");
        assert_eq!(headings[0], "Che cos'è rustcopy");
        assert!(headings.contains(&"Quando qualcosa non va"));
        assert!(headings.contains(&"Livelli di sicurezza"));
    }

    /// Every button the help names exists in the interface, spelled the same: a renamed button turns
    /// the guide into a lie, so this reads the screens' own text.
    #[test]
    fn the_buttons_it_names_exist_on_screen() {
        let screens = [
            include_str!("../ui/strings.slint"),
            include_str!("../ui/app.slint"),
            include_str!("../ui/sessions.slint"),
        ]
        .join("\n")
        .lines()
        .filter(|line| !line.trim_start().starts_with("//"))
        .collect::<Vec<_>>()
        .join("\n")
        .to_lowercase();
        let text: String = entries().iter().map(|e| e.text.clone()).collect();
        let mut named = 0;
        for part in text.split('«').skip(1) {
            let Some((label, _)) = part.split_once('»') else {
                continue;
            };
            // Only the names with a trailing "..." or capitalised single labels are buttons; the
            // quoted states ("Da controllare") are checked against the strings too.
            let bare = label.trim_end_matches("...");
            if bare.is_empty() || bare.contains(':') {
                continue;
            }
            named += 1;
            assert!(
                screens.contains(&bare.to_lowercase()),
                "the help names «{label}» but no screen has it"
            );
        }
        assert!(named > 20, "the check must actually look at the labels");
    }
}
