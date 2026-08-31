//! Deterministic suggestions derived from the run history (Fase 1 of `VALUTAZIONE_AI.md`).
//!
//! This module is the answer to "can rustcopy plan and optimise jobs on its own?" — and the answer
//! deliberately involves **no language model**. Choosing a schedule window, a retention depth or a
//! thread count, and spotting that a run went wrong, are questions about the distribution of past
//! runs. They are statistics, and statistics have three properties a backup tool actually needs:
//!
//! - **Offline.** Works inside a Windows service at 03:00 with no network and no API key.
//! - **Deterministic.** The same history yields the same advice, so it can be unit-tested.
//! - **Free.** No per-run cost, nothing to rate-limit.
//!
//! # Two rules this module holds itself to
//!
//! 1. **Never advise without showing the numbers.** Every [`Advice`] carries the evidence that
//!    produced it. An operator must be able to disagree with the reasoning, not just the verdict.
//! 2. **Never claim more than the sample supports.** With three runs there is no meaningful
//!    distribution; the honest output is "not enough data yet", not a confident number. Each check
//!    declares its own minimum and stays silent below it.
//!
//! # What this module must never do
//!
//! Advise, never act. It has no write path, takes `&RunHistory` and returns `Vec<Advice>`. The
//! prohibition list preserved in ROADMAP F61 — never expose `--force-purge`, unattended `--mirror`,
//! retention purges, or service/schedule installation to an automated caller — applies here in
//! full. Suggesting a retention depth is useful; applying one is the operator's call, because the
//! failure mode is deleted history.

use crate::history::{RunHistory, RunRecord};

/// Fewer runs than this and a duration distribution is noise, not a distribution.
const MIN_RUNS_FOR_TIMING: usize = 3;
/// Anomaly detection compares one run against the others; below this the "others" are too few.
const MIN_RUNS_FOR_ANOMALY: usize = 5;
/// A run this many robust deviations from the median is worth surfacing. 3.5 is the conventional
/// cutoff for the modified z-score.
const ANOMALY_THRESHOLD: f64 = 3.5;
/// Consistency factor making the median absolute deviation comparable to a standard deviation.
const MAD_TO_SIGMA: f64 = 0.6745;
/// An anomaly must also differ from the median by at least this fraction of it.
///
/// Statistical significance is not practical significance, and on a tight sample the two come
/// apart badly. Running the real binary produced a run of 0.10s against a median of 0.09s with a
/// modified z-score of 11.1 — arithmetically a huge deviation, operationally 10 milliseconds. A
/// detector that raises a warning for that gets ignored, and an ignored detector is worse than
/// none, because it also hides the real incident. Both gates must therefore pass.
const MIN_RELATIVE_DEVIATION: f64 = 0.25;
/// Below this, a run's throughput is dominated by fixed overhead (process start, prescan, report
/// write) rather than by transfer speed, so it says nothing useful about `--threads`. A heuristic,
/// but an empirical one: measured against the real binary, a 657-byte repeat sync reported
/// 0.016 MB/s on the same host that did 8.1 MB/s copying 480 KB moments earlier.
const MIN_BYTES_FOR_THROUGHPUT: u64 = 10 * 1024 * 1024;

/// What a piece of advice is about.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Topic {
    Schedule,
    Retention,
    Threads,
    Anomaly,
    Integrity,
    Sample,
}

impl Topic {
    pub fn label(self) -> &'static str {
        match self {
            Topic::Schedule => "schedulazione",
            Topic::Retention => "retention",
            Topic::Threads => "thread",
            Topic::Anomaly => "anomalia",
            Topic::Integrity => "integrità",
            Topic::Sample => "campione",
        }
    }
}

/// How much attention an item deserves. Deliberately not an error type: nothing here fails a run.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Severity {
    /// Context, not a recommendation.
    Info,
    /// A concrete, actionable proposal.
    Suggestion,
    /// Something looks wrong and a human should look.
    Warning,
}

impl Severity {
    pub fn label(self) -> &'static str {
        match self {
            Severity::Info => "INFO",
            Severity::Suggestion => "PROPOSTA",
            Severity::Warning => "ATTENZIONE",
        }
    }
}

/// One suggestion, with the numbers that produced it.
#[derive(Debug, Clone, PartialEq)]
pub struct Advice {
    pub topic: Topic,
    pub severity: Severity,
    pub headline: String,
    /// The measurements behind `headline`. Never empty for a `Suggestion` or a `Warning` — see the
    /// module docs: advice without evidence is not reviewable.
    pub evidence: Vec<String>,
}

impl Advice {
    fn new(
        topic: Topic,
        severity: Severity,
        headline: impl Into<String>,
        evidence: Vec<String>,
    ) -> Self {
        Self {
            topic,
            severity,
            headline: headline.into(),
            evidence,
        }
    }
}

/// Runs every check against `history` and returns what it found, most severe first.
///
/// An empty history yields a single `Info` telling the operator why there is nothing to say —
/// silence would be indistinguishable from a bug.
pub fn analyse(history: &RunHistory) -> Vec<Advice> {
    let mut out = Vec::new();

    if history.is_empty() {
        out.push(Advice::new(
            Topic::Sample,
            Severity::Info,
            "Nessuna run registrata: non c'è ancora storico su cui ragionare.",
            vec![
                "L'indice viene scritto a fine run in <dest>/.rustcopy_history.jsonl.".into(),
                "Esegui qualche backup e ripeti --advise.".into(),
            ],
        ));
        return out;
    }

    if history.skipped_lines() > 0 {
        out.push(Advice::new(
            Topic::Sample,
            Severity::Warning,
            format!(
                "{} righe dell'indice non sono leggibili e sono state ignorate.",
                history.skipped_lines()
            ),
            vec![
                "I suggerimenti qui sotto si basano quindi su un campione incompleto.".into(),
                "Una riga troncata è tipicamente un append interrotto: non è un problema per i backup già fatti.".into(),
            ],
        ));
    }

    let transfers = history.real_transfers();

    out.extend(schedule_advice(&transfers));
    out.extend(retention_advice(&transfers));
    out.extend(threads_advice(&transfers));
    out.extend(anomaly_advice(&transfers));
    out.extend(integrity_advice(&transfers));

    if out.is_empty() {
        out.push(Advice::new(
            Topic::Sample,
            Severity::Info,
            format!(
                "{} run in archivio, nessun rilievo: durate, throughput e integrità sono nella norma.",
                history.len()
            ),
            Vec::new(),
        ));
    }

    out.sort_by_key(|a| std::cmp::Reverse(a.severity));
    out
}

/// Duration envelope, and the shortest repeat interval that is actually safe.
fn schedule_advice(transfers: &[&RunRecord]) -> Vec<Advice> {
    if transfers.len() < MIN_RUNS_FOR_TIMING {
        return vec![Advice::new(
            Topic::Schedule,
            Severity::Info,
            format!(
                "Servono almeno {MIN_RUNS_FOR_TIMING} run reali per stimare una finestra di schedulazione (attuali: {}).",
                transfers.len()
            ),
            Vec::new(),
        )];
    }

    let mut durations: Vec<f64> = transfers.iter().map(|r| r.elapsed_seconds).collect();
    durations.sort_by(|a, b| a.partial_cmp(b).unwrap());

    let p50 = percentile(&durations, 0.50);
    let p95 = percentile(&durations, 0.95);
    let worst = *durations.last().expect("non-empty, checked above");

    // The safe repeat interval is the worst observed run plus headroom: a schedule that fires
    // again while the previous run is still copying gives two processes writing one destination.
    let safe_interval = worst * 1.5;

    vec![Advice::new(
        Topic::Schedule,
        Severity::Suggestion,
        format!(
            "Intervallo minimo consigliato fra due run: {}.",
            format_duration(safe_interval)
        ),
        vec![
            format!("Durata mediana su {} run: {}", durations.len(), format_duration(p50)),
            format!("95° percentile: {}", format_duration(p95)),
            format!("Peggiore osservata: {}", format_duration(worst)),
            "Il margine del 50% sulla peggiore evita che una run parta mentre la precedente sta ancora copiando."
                .to_string(),
            // Non suggerire una spec che --install-schedule rifiuterebbe: `hourly@N` vale solo
            // per N in 1..=23 (limite di schtasks.exe, verificato sul binario reale). Oltre, la
            // forma oraria non esiste e va proposta quella giornaliera.
            {
                let hours = (safe_interval / 3600.0).ceil().max(1.0) as u64;
                if hours <= 23 {
                    format!(
                        "Es. --install-schedule 'hourly@{hours}' — la forma oraria regge fino a 23."
                    )
                } else {
                    format!(
                        "Nessuna schedulazione oraria è adeguata: servono {} fra una run e                          l'altra, oltre il massimo di 23h. Usa 'daily@HH:MM'.",
                        format_duration(safe_interval)
                    )
                }
            },
        ],
    )]
}

/// Change rate, and what it implies for how much a retained generation costs.
fn retention_advice(transfers: &[&RunRecord]) -> Vec<Advice> {
    let generational: Vec<&&RunRecord> = transfers
        .iter()
        .filter(|r| r.backup_type.is_some())
        .collect();

    if generational.is_empty() {
        return vec![Advice::new(
            Topic::Retention,
            Severity::Info,
            "Nessuna run con --backup-type: la retention non si applica.",
            vec![
                "--keep-generations richiede --backup-type (full|incremental|differential).".into(),
            ],
        )];
    }

    let rates: Vec<f64> = generational
        .iter()
        .filter(|r| r.total_files > 0)
        .map(|r| r.files_copied as f64 / r.total_files as f64)
        .collect();

    if rates.is_empty() {
        return Vec::new();
    }

    let mut sorted = rates.clone();
    sorted.sort_by(|a, b| a.partial_cmp(b).unwrap());
    let median_rate = percentile(&sorted, 0.50);

    let median_total_bytes = {
        let mut bytes: Vec<f64> = generational.iter().map(|r| r.total_bytes as f64).collect();
        bytes.sort_by(|a, b| a.partial_cmp(b).unwrap());
        percentile(&bytes, 0.50)
    };

    // A retained cycle costs one full plus (N-1) deltas at the observed change rate.
    let cost_of = |generations: u32| -> f64 {
        median_total_bytes * (1.0 + median_rate * (generations.saturating_sub(1)) as f64)
    };

    vec![Advice::new(
        Topic::Retention,
        Severity::Suggestion,
        format!(
            "Il {:.1}% dei file cambia fra una run e l'altra: le generazioni incrementali costano poco.",
            median_rate * 100.0
        ),
        vec![
            format!("Campione: {} run con --backup-type", generational.len()),
            format!("Dimensione mediana dell'albero: {}", format_bytes(median_total_bytes)),
            format!("--keep-generations 3  → circa {}", format_bytes(cost_of(3))),
            format!("--keep-generations 7  → circa {}", format_bytes(cost_of(7))),
            format!("--keep-generations 14 → circa {}", format_bytes(cost_of(14))),
            "Stima sul tasso di variazione osservato; un cambio di contenuto la sposta.".into(),
            "La retention cancella dati: applicala tu, dopo aver verificato lo spazio disponibile.".into(),
        ],
    )]
}

/// Whether a different `--threads` has ever measurably helped.
fn threads_advice(transfers: &[&RunRecord]) -> Vec<Advice> {
    if transfers.len() < MIN_RUNS_FOR_TIMING {
        return Vec::new();
    }

    // Only runs that moved enough data may feed a throughput statistic.
    //
    // The naive filter (`bytes_copied > 0`) is not enough, as running the real binary showed: a
    // repeat sync over an already-aligned destination copied 657 bytes in 0.04s and reported
    // 0.016 MB/s on hardware that had just done 8.1 MB/s on the initial copy. That number
    // describes process startup, the prescan and the report write — fixed overhead — not transfer
    // speed, and averaging it in drags the median to zero. Excluding those runs is not cherry
    // picking: they carry no information about --threads at all.
    let moved: Vec<&&RunRecord> = transfers
        .iter()
        .filter(|record| record.bytes_copied >= MIN_BYTES_FOR_THROUGHPUT)
        .collect();

    if moved.len() < MIN_RUNS_FOR_TIMING {
        return vec![Advice::new(
            Topic::Threads,
            Severity::Info,
            format!(
                "Solo {} run hanno spostato dati a sufficienza per misurare il throughput.",
                moved.len()
            ),
            vec![
                format!(
                    "{} run su {} hanno copiato meno di {}: il loro throughput misura l'overhead fisso, non la velocità.",
                    transfers.len() - moved.len(),
                    transfers.len(),
                    format_bytes(MIN_BYTES_FOR_THROUGHPUT as f64)
                ),
                format!("Servono almeno {MIN_RUNS_FOR_TIMING} run significative per confrontare --threads."),
            ],
        )];
    }

    let mut by_threads: std::collections::BTreeMap<u16, Vec<f64>> = Default::default();
    for record in &moved {
        by_threads
            .entry(record.threads)
            .or_default()
            .push(record.throughput_mbps);
    }

    if by_threads.len() < 2 {
        let (threads, samples) = by_threads
            .iter()
            .next()
            .expect("non-empty, transfers checked above");
        let cpus = moved[0].logical_cpus;
        let mut sorted = samples.clone();
        sorted.sort_by(|a, b| a.partial_cmp(b).unwrap());
        return vec![Advice::new(
            Topic::Threads,
            Severity::Info,
            format!(
                "Tutte le run hanno usato --threads {threads}: non c'è confronto possibile."
            ),
            vec![
                format!("Throughput mediano: {:.1} MB/s", percentile(&sorted, 0.50)),
                format!("CPU logiche sull'host: {cpus}"),
                "Per capire se un valore diverso conviene, serve una run di prova con --threads diverso.".into(),
            ],
        )];
    }

    let mut ranked: Vec<(u16, f64, usize)> = by_threads
        .into_iter()
        .map(|(threads, mut samples)| {
            samples.sort_by(|a, b| a.partial_cmp(b).unwrap());
            let count = samples.len();
            (threads, percentile(&samples, 0.50), count)
        })
        .collect();
    ranked.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap());

    let (best_threads, best_median, best_count) = ranked[0];
    let evidence: Vec<String> = ranked
        .iter()
        .map(|(threads, median, count)| {
            format!("--threads {threads}: mediana {median:.1} MB/s su {count} run")
        })
        .collect();

    vec![Advice::new(
        Topic::Threads,
        Severity::Suggestion,
        format!(
            "Il throughput migliore osservato è con --threads {best_threads} ({best_median:.1} MB/s)."
        ),
        evidence
            .into_iter()
            .chain(std::iter::once(format!(
                "Il valore migliore poggia su {best_count} run: con un campione così piccolo la differenza può essere rumore."
            )))
            .collect(),
    )]
}

/// Whether the most recent run stands out against the others.
fn anomaly_advice(transfers: &[&RunRecord]) -> Vec<Advice> {
    if transfers.len() < MIN_RUNS_FOR_ANOMALY {
        return Vec::new();
    }

    let (latest, past) = transfers.split_last().expect("checked non-empty");
    let mut out = Vec::new();

    let checks: [(&str, f64, Vec<f64>, bool); 3] = [
        (
            "durata",
            latest.elapsed_seconds,
            past.iter().map(|r| r.elapsed_seconds).collect(),
            true,
        ),
        (
            "throughput",
            latest.throughput_mbps,
            past.iter().map(|r| r.throughput_mbps).collect(),
            false,
        ),
        (
            "file copiati",
            latest.files_copied as f64,
            past.iter().map(|r| r.files_copied as f64).collect(),
            true,
        ),
    ];

    for (name, value, sample, higher_is_worse) in checks {
        let Some(score) = modified_z_score(value, &sample) else {
            continue;
        };
        if score.abs() < ANOMALY_THRESHOLD {
            continue;
        }
        // Second gate: the difference must also be large in relative terms. See
        // MIN_RELATIVE_DEVIATION for the run that made this necessary.
        let mut ordered = sample.clone();
        ordered.sort_by(|a, b| a.partial_cmp(b).unwrap());
        let median = percentile(&ordered, 0.50);
        let relative = if median.abs() > f64::EPSILON {
            (value - median).abs() / median.abs()
        } else {
            f64::INFINITY
        };
        if relative < MIN_RELATIVE_DEVIATION {
            continue;
        }
        let direction = if score > 0.0 { "sopra" } else { "sotto" };
        let concerning = (score > 0.0) == higher_is_worse;
        let mut sorted = sample.clone();
        sorted.sort_by(|a, b| a.partial_cmp(b).unwrap());

        out.push(Advice::new(
            Topic::Anomaly,
            if concerning {
                Severity::Warning
            } else {
                Severity::Info
            },
            format!("L'ultima run ha un valore di {name} nettamente {direction} la norma."),
            vec![
                format!("Valore osservato: {}", format_metric(name, value)),
                format!(
                    "Mediana delle {} run precedenti: {}",
                    sample.len(),
                    format_metric(name, percentile(&sorted, 0.50))
                ),
                format!("Deviazione robusta (z modificato): {score:.1}"),
                format!("Scostamento relativo dalla mediana: {:.0}%", relative * 100.0),
                format!(
                    "Soglie: z modificato > {ANOMALY_THRESHOLD}, e almeno {:.0}% di differenza — devono valere entrambe.",
                    MIN_RELATIVE_DEVIATION * 100.0
                ),
            ],
        ));
    }

    out
}

/// Recurring integrity problems, which one run's exit code alone cannot reveal.
fn integrity_advice(transfers: &[&RunRecord]) -> Vec<Advice> {
    let failed: Vec<&&RunRecord> = transfers.iter().filter(|r| r.exit_code == 4).collect();
    if failed.is_empty() {
        return Vec::new();
    }

    let total_errors: usize = failed.iter().map(|r| r.integrity_errors).sum();
    let severity = if failed.len() > 1 {
        Severity::Warning
    } else {
        Severity::Suggestion
    };

    let mut evidence = vec![
        format!(
            "{} run su {} sono uscite con exit code 4 (copia riuscita, verifica fallita).",
            failed.len(),
            transfers.len()
        ),
        format!("Errori di integrità totali nel campione: {total_errors}"),
    ];
    if failed.len() > 1 {
        evidence
            .push("Un mismatch ricorrente non è transitorio: va guardato, non filtrato.".into());
        evidence.push(
            "Se invece sono file di log/temporanei, --ignore-transient-missing li esclude.".into(),
        );
    } else {
        evidence.push(
            "Un solo episodio può essere transitorio (file toccati durante la copia).".into(),
        );
    }

    vec![Advice::new(
        Topic::Integrity,
        severity,
        "La verifica di integrità ha già fallito su questo job.",
        evidence,
    )]
}

/// Linear-interpolated percentile over an already-sorted slice.
fn percentile(sorted: &[f64], p: f64) -> f64 {
    if sorted.is_empty() {
        return 0.0;
    }
    if sorted.len() == 1 {
        return sorted[0];
    }
    let rank = p * (sorted.len() - 1) as f64;
    let low = rank.floor() as usize;
    let high = rank.ceil() as usize;
    if low == high {
        return sorted[low];
    }
    let weight = rank - low as f64;
    sorted[low] * (1.0 - weight) + sorted[high] * weight
}

/// Modified z-score against a sample's median absolute deviation.
///
/// Chosen over a plain z-score deliberately: with a handful of runs, one pathological outlier
/// inflates the standard deviation enough to hide itself. The MAD is unaffected by it.
///
/// Returns `None` when the sample has no spread at all — every past run identical means there is
/// no scale against which to call anything anomalous, and dividing by zero would report every
/// value as infinitely unusual.
fn modified_z_score(value: f64, sample: &[f64]) -> Option<f64> {
    if sample.len() < 2 {
        return None;
    }
    let mut sorted = sample.to_vec();
    sorted.sort_by(|a, b| a.partial_cmp(b).unwrap());
    let median = percentile(&sorted, 0.50);

    let mut deviations: Vec<f64> = sample.iter().map(|v| (v - median).abs()).collect();
    deviations.sort_by(|a, b| a.partial_cmp(b).unwrap());
    let mad = percentile(&deviations, 0.50);

    if mad <= f64::EPSILON {
        return None;
    }
    Some(MAD_TO_SIGMA * (value - median) / mad)
}

fn format_metric(name: &str, value: f64) -> String {
    match name {
        "durata" => format_duration(value),
        "throughput" => format!("{value:.1} MB/s"),
        _ => format!("{value:.0}"),
    }
}

fn format_duration(seconds: f64) -> String {
    let seconds = seconds.max(0.0);
    // Short runs are real (a sync over a small tree finishes in milliseconds) and rounding them to
    // whole seconds produced useless output against the real binary — first "intervallo minimo
    // consigliato: 0s", then an anomaly reading "osservato <1s, mediana <1s" for a genuine 11-sigma
    // difference. Sub-10s values therefore keep two decimals: the comparison has to stay legible at
    // the scale where it was actually made.
    if seconds < 10.0 {
        return format!("{seconds:.2}s");
    }
    let total = seconds.round() as u64;
    let (h, m, s) = (total / 3600, (total % 3600) / 60, total % 60);
    if h > 0 {
        format!("{h}h {m:02}m")
    } else if m > 0 {
        format!("{m}m {s:02}s")
    } else {
        format!("{s}s")
    }
}

fn format_bytes(bytes: f64) -> String {
    const UNITS: [&str; 5] = ["B", "KB", "MB", "GB", "TB"];
    let mut value = bytes.max(0.0);
    let mut unit = 0;
    while value >= 1024.0 && unit < UNITS.len() - 1 {
        value /= 1024.0;
        unit += 1;
    }
    format!("{value:.1} {}", UNITS[unit])
}

/// Renders advice for the terminal.
pub fn render(advice: &[Advice], history: &RunHistory) -> String {
    let mut out = String::new();
    out.push_str(&format!(
        "rustcopy --advise — {} run in archivio\n",
        history.len()
    ));
    out.push_str(&"=".repeat(60));
    out.push('\n');

    for item in advice {
        out.push_str(&format!(
            "\n[{}] {} — {}\n",
            item.severity.label(),
            item.topic.label(),
            item.headline
        ));
        for line in &item.evidence {
            out.push_str(&format!("    · {line}\n"));
        }
    }

    out.push_str(
        "\nOgni proposta è derivata dai numeri sopra, senza modelli linguistici.\n\
         rustcopy suggerisce e non applica: le operazioni distruttive restano tue.\n",
    );
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;

    fn record(elapsed: f64, throughput: f64, threads: u16) -> RunRecord {
        RunRecord {
            timestamp: Utc::now(),
            job: None,
            source: "C:/src".into(),
            dest: "D:/dst".into(),
            exit_code: 0,
            total_files: 100,
            // Above MIN_BYTES_FOR_THROUGHPUT: the default fixture stands for a run large enough
            // that its throughput is meaningful. Tests about the threshold override it explicitly.
            total_bytes: 500 * 1024 * 1024,
            files_copied: 10,
            bytes_copied: 50 * 1024 * 1024,
            elapsed_seconds: elapsed,
            throughput_mbps: throughput,
            inventory_seconds: 1.0,
            transfer_seconds: elapsed - 1.0,
            verification_seconds: None,
            integrity_status: None,
            integrity_errors: 0,
            threads,
            logical_cpus: 8,
            backup_type: None,
            dry_run: false,
            report_path: None,
        }
    }

    fn history_of(records: Vec<RunRecord>) -> RunHistory {
        let raw: String = records
            .iter()
            .map(|r| format!("{}\n", serde_json::to_string(r).unwrap()))
            .collect();
        let dir = tempfile::tempdir().unwrap();
        let path = RunHistory::path_for(&dir.path().join("report.json"), None);
        std::fs::write(&path, raw).unwrap();
        RunHistory::load_recent(&dir.path().join("report.json"), None, 0).unwrap()
    }

    #[test]
    fn an_empty_history_explains_itself_rather_than_saying_nothing() {
        let advice = analyse(&RunHistory::default());
        assert_eq!(advice.len(), 1);
        assert_eq!(advice[0].topic, Topic::Sample);
        assert!(!advice[0].evidence.is_empty(), "must say what to do next");
    }

    #[test]
    fn timing_advice_is_withheld_below_the_declared_minimum() {
        let history = history_of(vec![record(10.0, 50.0, 8), record(12.0, 50.0, 8)]);
        let advice = analyse(&history);
        let schedule = advice
            .iter()
            .find(|a| a.topic == Topic::Schedule)
            .expect("schedule topic must still be reported");
        assert_eq!(
            schedule.severity,
            Severity::Info,
            "with 2 runs it must state insufficiency, not propose an interval"
        );
        assert!(schedule.headline.contains("almeno"));
    }

    #[test]
    fn the_suggested_interval_clears_the_worst_observed_run() {
        let history = history_of(vec![
            record(600.0, 50.0, 8),
            record(900.0, 50.0, 8),
            record(1200.0, 50.0, 8),
        ]);
        let advice = analyse(&history);
        let schedule = advice
            .iter()
            .find(|a| a.topic == Topic::Schedule && a.severity == Severity::Suggestion)
            .expect("three runs is enough to advise");
        // Worst is 1200s = 20m; +50% headroom = 1800s = 30m.
        assert!(
            schedule.headline.contains("30m"),
            "expected headroom above the worst run, got: {}",
            schedule.headline
        );
    }

    #[test]
    fn every_suggestion_and_warning_carries_its_evidence() {
        let history = history_of(vec![
            record(600.0, 50.0, 8),
            record(900.0, 40.0, 4),
            record(1200.0, 60.0, 8),
        ]);
        for item in analyse(&history) {
            if matches!(item.severity, Severity::Suggestion | Severity::Warning) {
                assert!(
                    !item.evidence.is_empty(),
                    "advice without evidence is not reviewable: {:?}",
                    item
                );
            }
        }
    }

    #[test]
    fn a_flat_history_reports_no_anomaly() {
        let history = history_of((0..8).map(|_| record(600.0, 50.0, 8)).collect());
        let advice = analyse(&history);
        assert!(
            !advice.iter().any(|a| a.topic == Topic::Anomaly),
            "identical runs have no spread, so nothing can be anomalous"
        );
    }

    #[test]
    fn a_run_that_takes_far_longer_than_the_others_is_flagged() {
        let mut records: Vec<RunRecord> = (0..8)
            .map(|i| record(600.0 + i as f64 * 5.0, 50.0, 8))
            .collect();
        records.push(record(9_000.0, 50.0, 8));

        let advice = analyse(&history_of(records));
        let anomaly = advice
            .iter()
            .find(|a| a.topic == Topic::Anomaly)
            .expect("a 15x duration must be caught");
        assert_eq!(anomaly.severity, Severity::Warning);
        assert!(anomaly.headline.contains("durata"));
    }

    #[test]
    fn a_statistically_extreme_but_operationally_trivial_difference_is_not_an_anomaly() {
        // The exact case the real binary produced: sub-second runs so consistent that the MAD is
        // microscopic, making a 10ms difference score above 11 sigma. Statistically extreme,
        // operationally nothing. Warning on it would train the operator to ignore the detector.
        // Slight run-to-run jitter, as the real samples had — an exactly flat sample has no MAD
        // at all and is already covered by `a_flat_history_reports_no_anomaly`.
        let mut records: Vec<RunRecord> = (0..8)
            .map(|i| record(0.089 + (i % 3) as f64 * 0.001, 50.0, 8))
            .collect();
        records.push(record(0.100, 50.0, 8));

        let advice = analyse(&history_of(records));

        assert!(
            !advice.iter().any(|a| a.topic == Topic::Anomaly),
            "an 11% difference must not be reported however extreme its z-score: {advice:#?}"
        );
    }

    #[test]
    fn a_difference_that_is_both_extreme_and_large_is_still_reported() {
        // Guards the gate from being so strict it suppresses everything: same tight sample, but a
        // difference no operator would call trivial.
        let mut records: Vec<RunRecord> = (0..8)
            .map(|i| record(0.089 + (i % 3) as f64 * 0.001, 50.0, 8))
            .collect();
        records.push(record(9.0, 50.0, 8));

        let anomaly = analyse(&history_of(records))
            .into_iter()
            .find(|a| a.topic == Topic::Anomaly)
            .expect("a 100x difference must survive both gates");
        assert_eq!(anomaly.severity, Severity::Warning);
        assert!(
            anomaly
                .evidence
                .iter()
                .any(|e| e.contains("Scostamento relativo")),
            "the reader must see both gates, not just the z-score"
        );
    }

    #[test]
    fn short_durations_keep_enough_precision_to_be_compared() {
        // "<1s vs <1s, deviation 11.1" was real output before this: the formatter hid the very
        // comparison the line was making.
        assert_eq!(format_duration(0.09), "0.09s");
        assert_eq!(format_duration(0.10), "0.10s");
        assert_ne!(format_duration(0.09), format_duration(0.10));
        assert_eq!(format_duration(1800.0), "30m 00s");
        assert_eq!(format_duration(3660.0), "1h 01m");
    }

    #[test]
    fn recurring_integrity_failures_are_a_warning_a_single_one_is_not() {
        let mut once = record(600.0, 50.0, 8);
        once.exit_code = 4;
        once.integrity_errors = 2;

        let single = analyse(&history_of(vec![
            record(600.0, 50.0, 8),
            once.clone(),
            record(600.0, 50.0, 8),
        ]));
        let found = single
            .iter()
            .find(|a| a.topic == Topic::Integrity)
            .expect("one failure is still worth reporting");
        assert_eq!(found.severity, Severity::Suggestion);

        let repeated = analyse(&history_of(vec![
            once.clone(),
            record(600.0, 50.0, 8),
            once,
        ]));
        let found = repeated
            .iter()
            .find(|a| a.topic == Topic::Integrity)
            .expect("two failures must be reported");
        assert_eq!(
            found.severity,
            Severity::Warning,
            "a recurring mismatch is not transitory"
        );
    }

    #[test]
    fn thread_advice_refuses_to_compare_when_every_run_used_the_same_value() {
        let history = history_of(vec![
            record(600.0, 50.0, 8),
            record(600.0, 52.0, 8),
            record(600.0, 48.0, 8),
        ]);
        let threads = analyse(&history)
            .into_iter()
            .find(|a| a.topic == Topic::Threads)
            .expect("must still say something");
        assert_eq!(threads.severity, Severity::Info);
        assert!(threads.headline.contains("non c'è confronto"));
    }

    #[test]
    fn thread_advice_ranks_by_median_throughput_when_values_differ() {
        let history = history_of(vec![
            record(600.0, 20.0, 4),
            record(600.0, 22.0, 4),
            record(600.0, 80.0, 16),
            record(600.0, 78.0, 16),
        ]);
        let threads = analyse(&history)
            .into_iter()
            .find(|a| a.topic == Topic::Threads)
            .unwrap();
        assert_eq!(threads.severity, Severity::Suggestion);
        assert!(
            threads.headline.contains("--threads 16"),
            "got: {}",
            threads.headline
        );
    }

    #[test]
    fn near_no_op_runs_are_excluded_from_the_throughput_sample() {
        // Reproduces what the real binary produced: one real copy followed by repeat syncs that
        // moved a few hundred bytes each and reported ~0.016 MB/s. Before the threshold existed,
        // the median collapsed to 0.0 MB/s and the advice was worse than useless.
        let big = record(600.0, 8.1, 8);
        let mut tiny = record(0.04, 0.016, 8);
        tiny.bytes_copied = 657;
        tiny.total_bytes = 657;

        let advice = analyse(&history_of(vec![big, tiny.clone(), tiny.clone(), tiny]));
        let threads = advice
            .into_iter()
            .find(|a| a.topic == Topic::Threads)
            .expect("must still report something about threads");

        assert_eq!(threads.severity, Severity::Info);
        assert!(
            threads.headline.contains("a sufficienza"),
            "must say the sample is too small to measure, got: {}",
            threads.headline
        );
        assert!(
            threads
                .evidence
                .iter()
                .any(|e| e.contains("overhead fisso")),
            "must explain why those runs were excluded rather than silently dropping them"
        );
    }

    #[test]
    fn retention_advice_says_it_does_not_apply_without_backup_type() {
        let history = history_of(vec![record(600.0, 50.0, 8), record(600.0, 50.0, 8)]);
        let retention = analyse(&history)
            .into_iter()
            .find(|a| a.topic == Topic::Retention)
            .unwrap();
        assert_eq!(retention.severity, Severity::Info);
        assert!(retention.headline.contains("Nessuna run con --backup-type"));
    }

    #[test]
    fn retention_cost_grows_with_the_number_of_generations_kept() {
        let mut records = Vec::new();
        for _ in 0..3 {
            let mut r = record(600.0, 50.0, 8);
            r.backup_type = Some("incremental".into());
            records.push(r);
        }
        let retention = analyse(&history_of(records))
            .into_iter()
            .find(|a| a.topic == Topic::Retention)
            .unwrap();
        assert_eq!(retention.severity, Severity::Suggestion);
        assert!(retention
            .evidence
            .iter()
            .any(|e| e.contains("keep-generations 7")));
        assert!(
            retention
                .evidence
                .iter()
                .any(|e| e.contains("applicala tu")),
            "must not imply rustcopy will purge on its own"
        );
    }

    #[test]
    fn dry_runs_never_enter_the_timing_sample() {
        let mut dry = record(1.0, 500.0, 8);
        dry.dry_run = true;
        let history = history_of(vec![
            dry.clone(),
            dry.clone(),
            dry,
            record(600.0, 50.0, 8),
            record(620.0, 50.0, 8),
        ]);
        let schedule = analyse(&history)
            .into_iter()
            .find(|a| a.topic == Topic::Schedule)
            .unwrap();
        assert_eq!(
            schedule.severity,
            Severity::Info,
            "only 2 real transfers, so timing advice must be withheld despite 5 records"
        );
    }

    #[test]
    fn unreadable_lines_are_surfaced_as_a_warning() {
        let dir = tempfile::tempdir().unwrap();
        let path = RunHistory::path_for(&dir.path().join("report.json"), None);
        std::fs::write(
            &path,
            format!(
                "{}\ngarbage\n",
                serde_json::to_string(&record(600.0, 50.0, 8)).unwrap()
            ),
        )
        .unwrap();
        let history = RunHistory::load_recent(&dir.path().join("report.json"), None, 0).unwrap();

        let sample = analyse(&history)
            .into_iter()
            .find(|a| a.topic == Topic::Sample && a.severity == Severity::Warning)
            .expect("an incomplete sample must be declared, not hidden");
        assert!(sample.headline.contains("1 righe"));
    }

    #[test]
    fn output_is_ordered_with_the_most_severe_first() {
        let mut broken = record(600.0, 50.0, 8);
        broken.exit_code = 4;
        let history = history_of(vec![broken.clone(), record(600.0, 50.0, 8), broken]);

        let advice = analyse(&history);
        assert_eq!(advice[0].severity, Severity::Warning);
    }

    #[test]
    fn percentile_interpolates_between_neighbours() {
        let sorted = vec![0.0, 10.0];
        assert!((percentile(&sorted, 0.5) - 5.0).abs() < 1e-9);
        assert!((percentile(&sorted, 0.0) - 0.0).abs() < 1e-9);
        assert!((percentile(&sorted, 1.0) - 10.0).abs() < 1e-9);
    }

    #[test]
    fn one_bad_run_in_the_past_cannot_mask_the_next_one() {
        // This is the whole reason the module uses a MAD-based score rather than a plain z-score.
        // The reference sample contains a past anomaly (9000s among ~600s runs). A standard
        // deviation absorbs that outlier and becomes enormous, so the *next* anomalous run scores
        // far below any sensible threshold and goes unreported. The MAD is unmoved by it.
        let sample = vec![600.0, 605.0, 610.0, 600.0, 9_000.0];
        let next_run = 700.0;

        let robust = modified_z_score(next_run, &sample).expect("the sample does have spread");
        assert!(
            robust.abs() > ANOMALY_THRESHOLD,
            "the MAD-based score must still flag it, got {robust}"
        );

        // Same numbers, the naive way, to show what was avoided.
        let mean = sample.iter().sum::<f64>() / sample.len() as f64;
        let variance = sample.iter().map(|v| (v - mean).powi(2)).sum::<f64>() / sample.len() as f64;
        let naive = (next_run - mean) / variance.sqrt();
        assert!(
            naive.abs() < ANOMALY_THRESHOLD,
            "a plain z-score was supposed to miss this; got {naive}"
        );
    }

    #[test]
    fn a_sample_with_no_spread_yields_no_score_instead_of_infinity() {
        // Every past run identical: there is no scale to judge against, and dividing by a zero MAD
        // would call every value infinitely unusual.
        assert_eq!(modified_z_score(50.0, &[10.0, 10.0, 10.0]), None);
        assert_eq!(modified_z_score(50.0, &[10.0]), None);
    }

    #[test]
    fn render_includes_the_evidence_not_just_the_headline() {
        let history = history_of(vec![
            record(600.0, 50.0, 8),
            record(900.0, 50.0, 8),
            record(1200.0, 50.0, 8),
        ]);
        let advice = analyse(&history);
        let text = render(&advice, &history);
        assert!(text.contains("run in archivio"));
        assert!(text.contains("·"), "evidence lines must be rendered");
        assert!(
            text.contains("suggerisce e non applica"),
            "the advise-never-act boundary must be stated in the output itself"
        );
    }
    /// `--advise` must not hand the operator a command that `--install-schedule` will refuse.
    /// A job slow enough to need more than 23h between runs has no valid `hourly@N`.
    #[test]
    fn no_hourly_spec_is_suggested_beyond_what_schtasks_accepts() {
        // ~20h runs: safe_interval is 1.5x that, so well past the 23h ceiling.
        let history = history_of(vec![
            record(70_000.0, 50.0, 8),
            record(72_000.0, 50.0, 8),
            record(74_000.0, 50.0, 8),
        ]);
        let schedule = analyse(&history)
            .into_iter()
            .find(|a| a.topic == Topic::Schedule && a.severity == Severity::Suggestion)
            .expect("three runs is enough to advise");

        let text = schedule.evidence.join(" ");
        // The message may *mention* the hourly form to say it does not apply; what it must never
        // do is hand over a concrete `hourly@N` that --install-schedule would reject.
        for n in 1..=200 {
            assert!(
                !text.contains(&format!("hourly@{n}")),
                "no concrete hourly spec may be offered above the 23h ceiling, got: {text}"
            );
        }
        assert!(
            text.contains("daily@"),
            "the daily form must be offered instead, got: {text}"
        );
    }

    /// And below the ceiling the hourly suggestion is still made, with a value schtasks accepts.
    #[test]
    fn an_hourly_spec_is_still_suggested_inside_the_range() {
        let history = history_of(vec![
            record(3_600.0, 50.0, 8),
            record(3_700.0, 50.0, 8),
            record(3_800.0, 50.0, 8),
        ]);
        let schedule = analyse(&history)
            .into_iter()
            .find(|a| a.topic == Topic::Schedule && a.severity == Severity::Suggestion)
            .expect("advice expected");
        let text = schedule.evidence.join(" ");
        assert!(text.contains("hourly@"), "got: {text}");
        for n in 24..=48 {
            assert!(
                !text.contains(&format!("hourly@{n}")),
                "out of range: {text}"
            );
        }
    }
}
