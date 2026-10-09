//! The text of the Aiuto page (CATALOGO_COMPORTAMENTI_GUI.md L28). The sentences about what the console
//! does live here; the meaning of an exit code is not written again: it comes from
//! `runner::exit_code_meaning`, the one place that knows it.

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
const EXIT_CODES: std::ops::RangeInclusive<u8> = 0..=6;

pub fn entries() -> Vec<HelpEntry> {
    let mut list = vec![
        heading("Da dove si comincia"),
        item(
            "Una copia al volo",
            "«Nuova copia»: scegli o trascina le cartelle, scegli dove metterle e premi Copia. Copia solo i file nuovi o cambiati e non cancella nulla.",
        ),
        item(
            "Controlla prima",
            "Conta file e dimensione di ciò che hai scelto e dice se la destinazione ha spazio. Lo fa solo quando lo chiedi: su un disco grande può richiedere del tempo.",
        ),
        item(
            "Salvare una copia come attività",
            "Dal dettaglio di una copia, «Salva come attività...»: nasce un'attività con una cartella sua, col suo report e il suo storico, che si rilancia con un clic. Non conserva mai mirror, pulizie, verifica o cifratura che non hai scelto tu.",
        ),
        item(
            "Un file di configurazione",
            "In Attività, «Aggiungi un file di configurazione...»: il file resta dove sta e non viene mai copiato né modificato. «Togli» lo toglie dall'elenco e lascia il file.",
        ),
        item(
            "Un report",
            "In Attività, «Apri un report...» mostra il report di una copia fatta altrove o da una pianificazione, e lo storico che gli sta accanto.",
        ),
        item(
            "Se non hai niente da provare",
            "In questa pagina, «Crea un esempio in Documenti» prepara pochi file finti e un file di configurazione già pronto, e lo aggiunge alle attività.",
        ),
        heading("Cosa fa ciascuna voce"),
        item(
            "Lavoro",
            "Ogni copia avviata da qui, con il suo esito. Una copia interrotta mostra «Riprendi» se ha lasciato un punto di ripresa.",
        ),
        item(
            "Proprietà",
            "Ogni impostazione di un file, raggruppata, con da dove viene il valore che vince e quali scelte hanno una conseguenza.",
        ),
        item(
            "Modifica",
            "Scrive una proposta in un file nuovo accanto all'originale: quello in uso non viene mai toccato, non si può accendere il mirror e non si può abbassare la conservazione.",
        ),
        item(
            "Storico",
            "Le esecuzioni passate con il loro esito, un filtro per esito, l'esportazione in CSV e le osservazioni calcolate dal motore.",
        ),
        item(
            "Credenziali",
            "Salva o toglie un segreto in Gestione credenziali di Windows. Lo si usa scrivendo keyring:NOME dove un campo accetta una chiave o una password.",
        ),
        heading("Parole che si incontrano"),
        item(
            "mirror",
            "Rende la destinazione identica alla sorgente e quindi cancella lì ciò che nella sorgente non c'è più. È l'impostazione più distruttiva: la console non può accenderla, va scritta a mano nel file.",
        ),
        item(
            "generazione, ciclo",
            "Con un tipo di copia scelto la copia diventa una storia: una completa più le incrementali o differenziali che la seguono formano un ciclo. La conservazione toglie cicli interi, mai una generazione sola.",
        ),
        item(
            "verifica",
            "Rilegge i file copiati e li confronta. Più sicura, più lenta. xxh3 rileva i danni ma non è crittografico: per dati che qualcuno potrebbe manomettere scegli SHA-256 o BLAKE3.",
        ),
        item(
            "ereditato",
            "In un file con più job, un valore non scritto nel job viene dai valori di primo livello. Le proprietà dicono per ogni voce da dove arriva.",
        ),
        heading("Come può finire una copia"),
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

    #[test]
    fn the_meaning_of_a_code_is_the_cores_not_a_copy() {
        let list = entries();
        let four = list.iter().find(|e| e.term == "4").expect("code 4");
        assert_eq!(four.text, runner::exit_code_meaning(4));
    }
}
